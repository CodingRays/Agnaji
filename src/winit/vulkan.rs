use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ash::vk;
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use crate::vulkan::InstanceContext;
use crate::vulkan::surface::{Surface, VulkanSurfaceCreateError, VulkanSurfaceProvider};
use crate::winit::window::{SurfaceBuildError, Window};

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
    fn suspended(&self) -> bool {
        self.window.get_backend().is_suspended()
    }

    fn wait_unsuspended(&self) {
        self.window.get_backend().wait_resumed()
    }

    fn create_surface<'a, 'b>(&'a self, instance: &'b InstanceContext) -> Result<Surface<'a, 'b>, VulkanSurfaceCreateError> {
        let (guard, surface) = self.window.try_build_surface(|window| {
            unsafe {
                ash_window::create_surface(instance.get_entry(), instance.get_instance(), window.raw_display_handle(), window.raw_window_handle(), None)
            }.map_err(|err| {
                log::error!("Failed to create vulkan surface for window: {:?}", err);
                VulkanSurfaceCreateError::VulkanError(err)
            })
        }).map_err(|err| {
            match err {
                SurfaceBuildError::SurfaceAlreadyExists => VulkanSurfaceCreateError::SurfaceAlreadyExists,
                SurfaceBuildError::Suspended => VulkanSurfaceCreateError::Suspended,
                SurfaceBuildError::BuildError(err) => err,
            }
        })?;

        Ok(Surface::new(instance, surface, Box::new(guard)))
    }

    fn get_canvas_size(&self) -> Option<Vec2u32> {
        Some(self.window.get_current_size())
    }
}