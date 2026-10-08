//! Global shortcuts through `org.freedesktop.portal.GlobalShortcuts`: one
//! session, created on first use, whose shortcuts are rebound on change.

use std::io;

use aegle_dbus::{Message, Value};

use super::{Call, Request, Shared, State as Desktop};
use crate::{Event, Shortcut};

const PORTAL: &str = "org.freedesktop.portal.GlobalShortcuts";

#[derive(Default)]
pub(super) struct State {
    /// The portal session, once created.
    session: Option<String>,
    /// Whether a CreateSession request is running.
    creating: bool,
    /// `(id, description, trigger)` to bind once the session exists.
    wanted: Vec<(String, String, String)>,
}

impl State {
    /// CreateSession failed; a later bind tries again.
    pub(super) fn failed(&mut self) {
        self.creating = false;
    }
}

fn call(member: &str, body: Vec<Value>) -> Message {
    Message::call(
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        PORTAL,
        member,
        body,
    )
}

pub(super) fn bind(shared: &Shared, shortcuts: &[Shortcut<'_>]) -> io::Result<()> {
    let mut state = shared.lock();
    state.shortcuts.wanted = shortcuts
        .iter()
        .map(|s| (s.id.into(), s.description.into(), s.trigger.xdg()))
        .collect();
    if state.shortcuts.session.is_some() {
        return send_bind(shared, &mut state);
    }
    if state.shortcuts.creating {
        return Ok(());
    }
    let id = state.id();
    let token = format!("aegle_session_{id}");
    let options = Value::dict([
        ("handle_token", Value::str(&token)),
        ("session_handle_token", Value::str(&token)),
    ]);
    let path = shared.request_path(&token);
    state.requests.insert(path.clone(), Request::Session);
    shared.call(
        &mut state,
        &call("CreateSession", vec![options]),
        Call::Request(path),
    )?;
    state.shortcuts.creating = true;
    Ok(())
}

fn send_bind(shared: &Shared, state: &mut Desktop) -> io::Result<()> {
    let session = state.shortcuts.session.clone().expect("a created session");
    let token = format!("aegle_bind_{}", state.id());
    let shortcuts = state
        .shortcuts
        .wanted
        .iter()
        .map(|(id, description, trigger)| {
            let options = Value::dict([
                ("description", Value::str(description)),
                ("preferred_trigger", Value::str(trigger)),
            ]);
            Value::Struct(vec![Value::str(id), options])
        });
    let body = vec![
        Value::Path(session),
        Value::Array("(sa{sv})".into(), shortcuts.collect()),
        Value::str(""),
        Value::dict([("handle_token", Value::str(&token))]),
    ];
    let path = shared.request_path(&token);
    state.requests.insert(path.clone(), Request::Bind);
    shared.call(state, &call("BindShortcuts", body), Call::Request(path))
}

/// Completes CreateSession, binding the wanted shortcuts, or BindShortcuts.
pub(super) fn response(
    shared: &Shared,
    state: &mut Desktop,
    request: Request,
    response: Option<u64>,
    results: &Value,
    events: &mut Vec<Event>,
) {
    let session = results.get("session_handle").and_then(Value::as_str);
    match (request, response, session) {
        (Request::Session, Some(0), Some(session)) => {
            state.shortcuts.creating = false;
            state.shortcuts.session = Some(session.into());
            if let Err(error) = send_bind(shared, state) {
                events.push(Event::Unavailable(format!("global shortcuts: {error}")));
            }
        }
        (Request::Session, ..) => {
            state.shortcuts.creating = false;
            events.push(Event::Unavailable("global shortcuts: no session".into()));
        }
        (_, Some(0), _) => {}
        _ => events.push(Event::Unavailable("global shortcuts: not bound".into())),
    }
}

pub(super) fn activated(state: &Desktop, message: &Message, events: &mut Vec<Event>) {
    let [session, id, ..] = &message.body[..] else {
        return;
    };
    if session.as_str() == state.shortcuts.session.as_deref()
        && let Some(id) = id.as_str()
    {
        events.push(Event::Shortcut { id: id.into() });
    }
}
