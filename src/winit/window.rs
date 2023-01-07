use std::sync::atomic::{AtomicBool, Ordering};
use winit::window::Window as WinitWindow;

use crate::prelude::*;

pub struct Window {
    window: WinitWindow,
    close_requested: AtomicBool,
}

impl Window {
    pub(in crate::winit) fn new(window: WinitWindow, initial_size: Vec2u32) -> Self {
        Self {
            window,
            close_requested: AtomicBool::new(false),
        }
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title)
    }

    pub fn is_close_requested(&self) -> bool {
        self.close_requested.load(Ordering::SeqCst)
    }

    pub(in crate::winit) fn signal_close_requested(&self) {
        self.close_requested.store(true, Ordering::SeqCst);
    }
}