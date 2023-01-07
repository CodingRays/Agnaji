use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use winit::window::Window as WinitWindow;

use crate::prelude::*;
use crate::winit::WinitBackend;

pub struct Window {
    backend: Arc<WinitBackend>,
    window: WinitWindow,
    close_requested: AtomicBool,
    state: Mutex<WindowState>,
}

impl Window {
    pub(in crate::winit) fn new(backend: Arc<WinitBackend>, window: WinitWindow, initial_size: Vec2u32) -> Self {
        Self {
            backend,
            window,
            close_requested: AtomicBool::new(false),
            state: Mutex::new(WindowState::new(initial_size)),
        }
    }

    pub fn get_backend(&self) -> &Arc<WinitBackend> {
        &self.backend
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title)
    }

    pub fn is_close_requested(&self) -> bool {
        self.close_requested.load(Ordering::SeqCst)
    }

    pub fn get_current_size(&self) -> Vec2u32 {
        self.state.lock().unwrap().size
    }

    pub(in crate::winit) fn on_close_requested(&self) {
        self.close_requested.store(true, Ordering::SeqCst);
    }

    pub(in crate::winit) fn on_resize(&self, new_size: Vec2u32) {
        self.state.lock().unwrap().size = new_size;
    }
}

struct WindowState {
    size: Vec2u32,
}

impl WindowState {
    fn new(initial_size: Vec2u32) -> Self {
        Self {
            size: initial_size
        }
    }
}