use crate::{native::App, platform::WakeHandle};
use aegle_ui::Result;
use std::{
    cell::RefCell,
    collections::VecDeque,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

/// Most messages a proxy holds before [`UiProxy::send`] rejects more.
const CAPACITY: usize = 1024;

struct Shared<T> {
    queue: Mutex<VecDeque<T>>,
    wake: WakeHandle,
    closed: AtomicBool,
}

/// Sends messages from any thread to a handler running on the UI thread.
///
/// The handler runs inside the event loop, outside every UI borrow, so it can use
/// control handles freely. Messages are delivered in order; wakes coalesce.
pub struct UiProxy<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Clone for UiProxy<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T> UiProxy<T> {
    /// Queues a message and wakes the event loop. Returns it back when 1024
    /// messages are already waiting or the application has exited.
    pub fn send(&self, message: T) -> std::result::Result<(), T> {
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(message);
        }
        {
            let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            if queue.len() >= CAPACITY {
                return Err(message);
            }
            queue.push_back(message);
        }
        self.shared.wake.wake();
        Ok(())
    }
}

/// UI-thread end: drains the queue into the application's handler.
pub(crate) struct Receiver<T> {
    shared: Arc<Shared<T>>,
    handler: Box<dyn FnMut(T) -> Result>,
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

/// Type-erased drain for the runtime's proxy list.
pub(crate) trait Drain {
    /// Handles queued messages, passing each handler result to `report`.
    fn drain(&mut self, report: &mut dyn FnMut(Result) -> Result) -> Result;
}

impl<T> Drain for Receiver<T> {
    fn drain(&mut self, report: &mut dyn FnMut(Result) -> Result) -> Result {
        // Snapshot the queue so a handler that sends to itself runs next wake.
        let batch =
            std::mem::take(&mut *self.shared.queue.lock().unwrap_or_else(|e| e.into_inner()));
        batch
            .into_iter()
            .try_for_each(|message| report((self.handler)(message)))
    }
}

impl App {
    /// Creates a thread-safe sender whose messages `handler` receives on the UI
    /// thread, in order, while the event loop runs. The handler may keep control
    /// handles, which stay on this thread. Handler errors go to
    /// [`App::on_error`], like a click callback's.
    pub fn proxy<T: Send + 'static>(
        &self,
        handler: impl FnMut(T) -> Result + 'static,
    ) -> Result<UiProxy<T>> {
        let mut runtime = self.runtime.borrow_mut();
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            wake: runtime.backend.wake_handle()?,
            closed: AtomicBool::new(false),
        });
        runtime.proxies.push(Rc::new(RefCell::new(Receiver {
            shared: shared.clone(),
            handler: Box::new(handler),
        })));
        Ok(UiProxy { shared })
    }

    /// Runs every proxy handler once, outside the runtime borrow, reporting
    /// handler errors to [`App::on_error`].
    pub(crate) fn drain_proxies(&self) -> Result {
        let proxies = self.runtime.borrow().proxies.clone();
        proxies
            .iter()
            .try_for_each(|proxy| proxy.borrow_mut().drain(&mut |result| self.report(result)))
    }
}

#[cfg(feature = "desktop-services")]
impl App {
    /// Connects to the desktop's services as `app_id`, a reverse-DNS name
    /// such as `org.example.Editor`, for file dialogs, notifications, a tray
    /// icon and global shortcuts. `handler` receives their events on the UI
    /// thread, like a [`Self::proxy`] handler. Fails without a desktop
    /// session, such as a Linux session without a bus.
    pub fn desktop(
        &self,
        app_id: &str,
        handler: impl FnMut(aegle_desktop::Event) -> Result + 'static,
    ) -> Result<aegle_desktop::Desktop> {
        let proxy = self.proxy(handler)?;
        let desktop = aegle_desktop::Desktop::new(app_id, move |event| {
            // Past 1024 waiting events the UI thread is stalled; drop the rest.
            let _ = proxy.send(event);
        })?;
        Ok(desktop)
    }
}
