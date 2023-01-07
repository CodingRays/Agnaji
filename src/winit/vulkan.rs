use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ash::vk;

use crate::vulkan::InstanceContext;
use crate::vulkan::surface::{VulkanSurfaceCreateError, VulkanSurfaceProvider};
use crate::winit::window::Window;

use crate::prelude::*;

pub struct WinitVulkanSurfaceProvider {
    window: Arc<Window>,
    has_surface: AtomicBool,
}

impl VulkanSurfaceProvider for WinitVulkanSurfaceProvider {
    fn suspended(&self) -> bool {
        self.window.get_backend().is_suspended()
    }

    fn wait_unsuspended(&self) {
        if self.has_surface.load(Ordering::SeqCst) {
            panic!("Called wait_unsuspended while surface exists");
        }
        self.window.get_backend().wait_resumed()
    }

    fn create_surface<'a>(&'a self, instance: &InstanceContext) -> Result<(vk::SurfaceKHR, Box<dyn FnOnce() + Send + 'a>), VulkanSurfaceCreateError> {
        if self.has_surface.load(Ordering::SeqCst) {
            panic!("Called create_surface while surface exists");
        }

        let mut surface = Err(VulkanSurfaceCreateError::Suspended);
        self.window.get_backend().with_client_api_guard_inc(|suspended| {
            if !suspended {
                surface = Ok(vk::SurfaceKHR::null());
                true
            } else {
                surface = Err(VulkanSurfaceCreateError::Suspended);
                false
            }
        });

        match surface {
            Ok(surface) => {
                Ok((surface, Box::new(|| {
                    self.window.get_backend().dec_client_api_count();
                    self.has_surface.store(false, Ordering::SeqCst);
                })))
            },
            Err(err) => {
                Err(err)
            }
        }
    }

    fn get_canvas_size(&self) -> Option<Vec2u32> {
        Some(self.window.get_current_size())
    }
}