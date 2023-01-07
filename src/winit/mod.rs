mod worker;
mod window;

use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event_loop::EventLoopProxy;

use crate::prelude::*;
use crate::winit::window::Window;
use crate::winit::worker::WindowChannel;

const DEFAULT_LOG_TARGET: &'static str = "agnaji::winit";

pub struct WinitBackend {
    event_loop_proxy: Mutex<EventLoopProxy<AgnajiEvent>>,
    quit_requested: AtomicBool,
    window_channel: WindowChannel,
}

impl WinitBackend {
    fn new(event_loop_proxy: EventLoopProxy<AgnajiEvent>) -> Self {
        Self {
            event_loop_proxy: Mutex::new(event_loop_proxy),
            quit_requested: AtomicBool::new(false),
            window_channel: WindowChannel::new(),
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

    fn push_event(&self, event: AgnajiEvent) {
        let result = self.event_loop_proxy.lock().unwrap().send_event(event);
        // Make sure we panic outside the mutex
        result.unwrap();
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

#[derive(Debug)]
enum AgnajiEvent {
    CreateWindow {
        id: u64,
        title: String,
        initial_size: Option<Vec2u32>,
    },
    Quit,
}
