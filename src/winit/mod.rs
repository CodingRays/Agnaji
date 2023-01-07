mod worker;
mod window;
mod vulkan;

use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
use static_assertions::assert_impl_all;
use winit::event_loop::EventLoopProxy;

use crate::prelude::*;
use crate::winit::window::Window;
use crate::winit::worker::WindowChannel;

const DEFAULT_LOG_TARGET: &'static str = "agnaji::winit";

pub struct WinitBackend {
    event_loop_proxy: Mutex<EventLoopProxy<AgnajiEvent>>,
    quit_requested: AtomicBool,
    window_channel: WindowChannel,

    /// Number of windows with a client api that need to be destroyed before a suspended event can
    /// return.
    client_api_count: Mutex<u32>,

    /// Will be waited on by the event loop thread if a suspend event is triggered. Needs to be
    /// signalled once all client apis have been destroyed.
    loop_wait_condvar: Condvar,

    /// Is signalled once suspended state changes. The `client_api_count` mutex should be used for
    /// waiting.
    ///
    /// Note: for correctness if one wants to wait for suspended state to change the mutex should be
    /// locked before checking the state variable and then use that same guard to wait to ensure no
    /// state change is missed.
    suspended_condvar: Condvar,

    /// If the backend is currently suspended.
    suspended_state: AtomicBool,
}

impl WinitBackend {
    fn new(event_loop_proxy: EventLoopProxy<AgnajiEvent>) -> Self {
        Self {
            event_loop_proxy: Mutex::new(event_loop_proxy),
            quit_requested: AtomicBool::new(false),
            window_channel: WindowChannel::new(),

            client_api_count: Mutex::new(0),
            suspended_state: AtomicBool::new(false),
            suspended_condvar: Condvar::new(),
            loop_wait_condvar: Condvar::new(),
        }
    }

    pub fn quit(&self) {
        if self.quit_requested.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.push_event(AgnajiEvent::Quit);
            log::debug!(target: DEFAULT_LOG_TARGET, "Submitted quit request");
        } else {
            log::debug!(target: DEFAULT_LOG_TARGET, "Quit request inhibited. (Already submitted request before)");
        }
    }

    pub fn create_window(&self, title: String, initial_size: Option<Vec2u32>) -> Result<Arc<Window>, String> {
        let id = self.window_channel.allocate_id();

        log::debug!(target: DEFAULT_LOG_TARGET, "Submitted window creation request: {:?} size: {:?} (RequestID: {})", &title, initial_size, id);
        self.push_event(AgnajiEvent::CreateWindow {
            id,
            title,
            initial_size,
        });

        self.window_channel.wait_ready(id).map_err(|err| {
            err.to_string()
        })
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended_state.load(Ordering::SeqCst)
    }

    /// Waits until the application is not suspended.
    pub fn wait_resumed(&self) {
        let mut guard = self.client_api_count.lock().unwrap();
        loop {
            if !self.suspended_state.load(Ordering::SeqCst) {
                return;
            }

            guard = self.suspended_condvar.wait(guard).unwrap();
        }
    }

    fn push_event(&self, event: AgnajiEvent) {
        let result = self.event_loop_proxy.lock().unwrap().send_event(event);
        // Make sure we panic outside the mutex
        result.unwrap();
    }

    /// Executes the provided function while holding the client api count guard. The current
    /// suspended state is passed as a parameter to the function. If the function returns true the
    /// count is incremented by 1. Otherwise it is left untouched.
    ///
    /// Note: Extra care must be taken to ensure the function will not panic as that would cause
    /// the client api count mutex to be poisoned.
    fn with_client_api_guard_inc<F>(&self, f: F) where F: FnOnce(bool) -> bool {
        let mut guard = self.client_api_count.lock().unwrap();
        if f(self.suspended_state.load(Ordering::SeqCst)) {
            *guard += 1;
        }
        drop(guard)
    }

    /// Decrements the client api count and notifies the loop wait condvar once it reaches 0.
    fn dec_client_api_count(&self) {
        let mut guard = self.client_api_count.lock().unwrap();
        *guard -= 1;
        if *guard == 0 {
            self.loop_wait_condvar.notify_all();
        }
        drop(guard);
    }

    /// Called by the event loop to signal that a suspended event has been received. Will set the
    /// suspended state to [`true`] and then wait for the client api count to reach 0 before
    /// returning.
    fn event_loop_signal_suspended(&self) {
        // Need to lock the guard before changing the suspended state to make sure any waiting threads get notified
        let mut guard = self.client_api_count.lock().unwrap();

        self.suspended_state.store(true, Ordering::SeqCst);
        self.suspended_condvar.notify_all();

        loop {
            let count = *guard;
            if count == 0 {
                return;
            }

            log::debug!(target: worker::EVENT_LOOP_LOG_TARGET, "Waiting for client api count to reach 0. Current: {}", count);
            guard = self.loop_wait_condvar.wait(guard).unwrap();
        }
    }

    /// Called by the event loop to signal that a resumed event has been received. Will set the
    /// suspended state to [`false`].
    fn event_loop_signal_resumed(&self) {
        // Need to lock the guard before changing the suspended state to make sure any waiting threads get notified
        let mut guard = self.client_api_count.lock().unwrap();

        self.suspended_state.store(false, Ordering::SeqCst);
        self.suspended_condvar.notify_all();

        drop(guard);
    }
}

pub fn run<F>(post_init: F) where F: FnOnce(Arc<WinitBackend>) + Send + UnwindSafe + 'static {
    worker::run(post_init)
}

// Required because condvar
impl UnwindSafe for WinitBackend {
}
impl RefUnwindSafe for WinitBackend {
}

assert_impl_all!(WinitBackend: Send, Sync);

#[derive(Debug)]
enum AgnajiEvent {
    CreateWindow {
        id: u64,
        title: String,
        initial_size: Option<Vec2u32>,
    },
    Quit,
}
