use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window as WinitWindow;

use crate::prelude::*;
use crate::vulkan::surface::VulkanSurfaceProvider;
use crate::winit::vulkan::WinitVulkanSurfaceProvider;
use crate::winit::WinitBackend;

pub struct Window {
    backend: Arc<WinitBackend>,
    window: WinitWindow,
    close_requested: AtomicBool,
    state: Mutex<WindowState>,

    has_surface: Mutex<bool>,
}

impl Window {
    pub(in crate::winit) fn new(backend: Arc<WinitBackend>, window: WinitWindow, initial_size: Vec2u32) -> Self {
        Self {
            backend,
            window,
            close_requested: AtomicBool::new(false),
            state: Mutex::new(WindowState::new(initial_size)),
            has_surface: Mutex::new(false),
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

    pub fn as_vulkan_surface_provider(self: &Arc<Self>) -> Box<dyn VulkanSurfaceProvider> {
        Box::new(WinitVulkanSurfaceProvider::new(self.clone()))
    }

    pub fn try_build_surface<F, S, E>(&self, f: F) -> Result<(SurfaceGuard, S), SurfaceBuildError<E>> where F: FnOnce(&WinitWindow) -> Result<S, E>, E: Clone + Debug {
        let mut guard = self.has_surface.lock().unwrap();
        if *guard == true {
            return Err(SurfaceBuildError::SurfaceAlreadyExists)
        }

        let mut result = Err(SurfaceBuildError::SurfaceAlreadyExists);
        self.backend.with_surface_count_guard_inc(|suspended| {
            if suspended {
                result = Err(SurfaceBuildError::Suspended);
                false
            } else {
                result = f(&self.window).map_err(SurfaceBuildError::BuildError);
                result.is_ok()
            }
        });

        let surface = result?;

        *guard = true;
        drop(guard);

        Ok((SurfaceGuard {
            window: self,
        }, surface))
    }

    pub(in crate::winit) fn on_close_requested(&self) {
        self.close_requested.store(true, Ordering::SeqCst);
    }

    pub(in crate::winit) fn on_resize(&self, new_size: Vec2u32) {
        self.state.lock().unwrap().size = new_size;
    }
}

unsafe impl HasRawWindowHandle for Window {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.window.raw_window_handle()
    }
}

unsafe impl HasRawDisplayHandle for Window {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        self.window.raw_display_handle()
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

#[derive(Clone, Debug)]
pub enum SurfaceBuildError<E: Clone + Debug> {
    SurfaceAlreadyExists,
    Suspended,
    BuildError(E),
}

pub struct SurfaceGuard<'a> {
    window: &'a Window,
}

impl<'a> Drop for SurfaceGuard<'a> {
    fn drop(&mut self) {
        let mut guard = self.window.has_surface.lock().unwrap();
        *guard = false;
        self.window.backend.dec_surface_count();
        drop(guard);
    }
}

impl<'a> crate::vulkan::surface::SurfaceGuard for SurfaceGuard<'a> {
}