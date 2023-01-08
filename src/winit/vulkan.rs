use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ash::vk;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::vulkan::InstanceContext;
use crate::vulkan::surface::{Surface, VulkanSurfaceProvider};
use crate::winit::window::Window;

use crate::prelude::*;

pub struct WinitVulkanSurfaceProvider {
    window: Arc<Window>,
}

impl WinitVulkanSurfaceProvider {
    pub(in crate::winit) fn new(window: Arc<Window>) -> Self {
        Self {
            window,
        }
    }
}

impl VulkanSurfaceProvider for WinitVulkanSurfaceProvider {
    fn create_surface<'a, 'b>(&'a self, instance: &'b InstanceContext) -> Result<Surface<'a, 'b>, vk::Result> {
        let surface = unsafe {
            ash_window::create_surface(
                instance.get_entry(),
                instance.get_instance(),
                self.window.get_window().raw_display_handle(),
                self.window.get_window().raw_window_handle(),
                None)
        }?;

        Ok(Surface::new(instance, surface))
    }

    fn get_canvas_size(&self) -> Option<Vec2u32> {
        Some(self.window.get_current_size())
    }
}