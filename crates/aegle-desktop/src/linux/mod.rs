//! Linux: everything over the session bus, read by one thread.

mod files;
mod shortcuts;
mod tray;

use std::{
    collections::HashMap,
    io,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex, MutexGuard},
    thread::JoinHandle,
    time::Duration,
};

use aegle_dbus::{Connection, Kind, Message, Value};

use crate::{Event, FileDialog, Notification, Shortcut, Tray};

pub(crate) type Sink = Box<dyn Fn(Event) + Send + Sync>;

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

/// What a sent call's return completes.
enum Call {
    /// A portal request whose Response is expected at the given path; the
    /// return names the actual path.
    Request(String),
    /// A notification and our id for it.
    Notify(u32),
    /// The tray's registration with the watcher.
    Register,
}

/// What a portal request's Response completes.
enum Request {
    Files(u32),
    Session,
    Bind,
}

#[derive(Default)]
struct State {
    next: u32,
    calls: HashMap<u32, Call>,
    requests: HashMap<String, Request>,
    /// The notification server's ids, to ours.
    notifications: HashMap<u32, u32>,
    tray: tray::State,
    shortcuts: shortcuts::State,
}

pub(crate) struct Shared {
    bus: Connection,
    app_id: String,
    sink: Sink,
    state: Mutex<State>,
}

pub(crate) struct Desktop {
    shared: Arc<Shared>,
    /// Shut down on drop to end the reading thread.
    socket: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Desktop {
    pub fn new(app_id: &str, sink: Sink) -> io::Result<Self> {
        let bus = Connection::session(Duration::from_secs(2))?;
        for rule in [
            "type='signal',interface='org.freedesktop.portal.Request',member='Response'",
            "type='signal',interface='org.freedesktop.Notifications'",
            "type='signal',interface='org.freedesktop.portal.GlobalShortcuts',member='Activated'",
            "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',\
             arg0='org.kde.StatusNotifierWatcher'",
        ] {
            bus.add_match(rule)?;
        }
        let mut incoming = bus.incoming()?;
        let socket = incoming.stream().try_clone()?;
        let shared = Arc::new(Shared {
            bus,
            app_id: app_id.into(),
            sink,
            state: Mutex::default(),
        });
        let reader = shared.clone();
        let thread = std::thread::Builder::new()
            .name("aegle-desktop".into())
            .spawn(move || {
                while let Ok(message) = incoming.recv() {
                    reader.handle(message);
                }
            })?;
        Ok(Self {
            shared,
            socket,
            thread: Some(thread),
        })
    }

    pub fn file_dialog(&self, dialog: &FileDialog<'_>, save: bool) -> io::Result<u32> {
        files::request(&self.shared, dialog, save)
    }

    pub fn notify(&self, notification: &Notification<'_>) -> io::Result<u32> {
        let shared = &self.shared;
        let actions = notification
            .actions
            .iter()
            .flat_map(|&(key, label)| [key, label]);
        let call = Message::call(
            NOTIFICATIONS,
            "/org/freedesktop/Notifications",
            NOTIFICATIONS,
            "Notify",
            vec![
                Value::str(&shared.app_id),
                Value::U32(0),
                Value::str(""),
                Value::str(notification.summary),
                Value::str(notification.body),
                Value::strings(actions),
                Value::dict([]),
                Value::I32(-1),
            ],
        );
        let mut state = shared.lock();
        let id = state.id();
        shared.call(&mut state, &call, Call::Notify(id))?;
        Ok(id)
    }

    pub fn set_tray(&self, tray: Option<&Tray<'_>>) -> io::Result<()> {
        tray::set(&self.shared, tray)
    }

    pub fn bind_shortcuts(&self, shortcuts: &[Shortcut<'_>]) -> io::Result<()> {
        shortcuts::bind(&self.shared, shortcuts)
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl State {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends `message` and records what its return completes, holding the
    /// state so the return cannot be handled first.
    fn call(&self, state: &mut State, message: &Message, call: Call) -> io::Result<()> {
        let serial = self.bus.send(message)?;
        state.calls.insert(serial, call);
        Ok(())
    }

    /// The object path where the portal will answer a request made with
    /// `token`.
    fn request_path(&self, token: &str) -> String {
        let sender = self.bus.name().trim_start_matches(':').replace('.', "_");
        format!("/org/freedesktop/portal/desktop/request/{sender}/{token}")
    }

    fn handle(&self, message: Message) {
        let mut events = Vec::new();
        {
            let mut state = self.lock();
            match message.kind {
                Kind::Return | Kind::Error => self.answered(&mut state, &message, &mut events),
                Kind::Signal => self.signal(&mut state, &message, &mut events),
                Kind::Call => tray::serve(self, &mut state, &message, &mut events),
            }
        }
        events.into_iter().for_each(|event| (self.sink)(event));
    }

    fn answered(&self, state: &mut State, message: &Message, events: &mut Vec<Event>) {
        let Some(call) = state.calls.remove(&message.reply_serial) else {
            return;
        };
        let failure = (message.kind == Kind::Error).then(|| {
            let text = message.body.first().and_then(Value::as_str).unwrap_or("");
            format!("{}: {text}", message.error)
        });
        match (call, failure) {
            (Call::Request(expected), None) => {
                // Older portals answer at another path than predicted.
                let actual = message.body.first().and_then(Value::as_str);
                if let Some(actual) = actual.filter(|path| *path != expected)
                    && let Some(request) = state.requests.remove(&expected)
                {
                    state.requests.insert(actual.into(), request);
                }
            }
            (Call::Request(expected), Some(failure)) => {
                match state.requests.remove(&expected) {
                    Some(Request::Files(request)) => events.push(Event::Files {
                        request,
                        paths: None,
                    }),
                    Some(Request::Session) => state.shortcuts.failed(),
                    _ => {}
                }
                events.push(Event::Unavailable(failure));
            }
            (Call::Notify(ours), None) => {
                if let Some(id) = message.body.first().and_then(Value::as_u64) {
                    state.notifications.insert(id as u32, ours);
                }
            }
            (Call::Notify(_), Some(failure)) => events.push(Event::Unavailable(failure)),
            (Call::Register, None) => {}
            (Call::Register, Some(failure)) => {
                events.push(Event::Unavailable(format!("tray: {failure}")))
            }
        }
    }

    fn signal(&self, state: &mut State, message: &Message, events: &mut Vec<Event>) {
        if message.is_signal("org.freedesktop.portal.Request", "Response") {
            let Some(request) = state.requests.remove(&message.path) else {
                return;
            };
            let (response, results) = match &message.body[..] {
                [response, results] => (response.as_u64(), results),
                _ => return,
            };
            match request {
                Request::Files(request) => {
                    let paths = (response == Some(0)).then(|| files::paths(results));
                    events.push(Event::Files { request, paths });
                }
                Request::Session | Request::Bind => {
                    shortcuts::response(self, state, request, response, results, events)
                }
            }
        } else if message.interface == NOTIFICATIONS {
            let id = message.body.first().and_then(Value::as_u64).unwrap_or(0) as u32;
            let Some(&notification) = state.notifications.get(&id) else {
                return;
            };
            match message.member.as_str() {
                "ActionInvoked" => {
                    let action = message.body.get(1).and_then(Value::as_str);
                    events.push(Event::NotificationAction {
                        notification,
                        action: action.unwrap_or("default").into(),
                    });
                }
                "NotificationClosed" => {
                    state.notifications.remove(&id);
                    events.push(Event::NotificationClosed { notification });
                }
                _ => {}
            }
        } else if message.is_signal("org.freedesktop.portal.GlobalShortcuts", "Activated") {
            shortcuts::activated(state, message, events);
        } else if message.is_signal("org.freedesktop.DBus", "NameOwnerChanged") {
            // A new watcher forgets earlier items.
            let owner = message.body.get(2).and_then(Value::as_str);
            if owner.is_some_and(|owner| !owner.is_empty()) && state.tray.shown() {
                let _ = tray::register(self, state);
            }
        }
    }
}
