use std::ffi::CString;
use std::marker::PhantomData;

use ash::vk;
use static_assertions::assert_impl_all;
use crate::utils::define_counting_id_type;

use crate::prelude::*;

define_counting_id_type!(pub, SurfaceProviderId);

/// Provides a api to create and use vulkan surfaces associated with some canvas (for example a
/// window).
pub trait VulkanSurfaceProvider: Send {
    /// Creates a new surface.
    ///
    /// # Safety
    /// Calling this function while a surface already exists in undefined behaviour.
    unsafe fn create_surface<'a, 'b>(&'a self, instance: &'b crate::vulkan::InstanceContext) -> Result<Surface<'a, 'b>, vk::Result>;

    /// Returns the size of the canvas in pixels backing the surface (for example the window size)
    /// or [`None`] if that is currently undefined. If [`None`] is returned the renderer may not
    /// be able to create a swapchain so during normal use this function should return a valid size.
    fn get_canvas_size(&self) -> Option<Vec2u32>;
}

/// Wrapper of a vulkan surface.
///
/// Ensures the struct backing the surface stays alive using the `'a` lifetime and automatically
/// destroys the surface when this struct is dropped.
pub struct Surface<'a, 'b> {
    instance: &'b crate::vulkan::InstanceContext,
    surface: vk::SurfaceKHR,

    #[allow(unused)]
    _phantom: PhantomData<&'a ()>
}

impl<'a, 'b> Surface<'a, 'b> {
    /// Creates a new instance of this struct for the provided surface.
    pub fn new(instance: &'b crate::vulkan::InstanceContext, surface: vk::SurfaceKHR) -> Self {
        if instance.get_khr_surface().is_none() {
            panic!("Called Surface::new with instance that does not have the VK_KHR_surface extension enabled");
        }
        if surface == vk::SurfaceKHR::null() {
            panic!("Called Surface::new with null surface");
        }

        Self {
            instance,
            surface,
            _phantom: PhantomData,
        }
    }

    /// Returns the vulkan surface handle-
    ///
    /// # Safety
    /// The surface will be destroyed when this struct is dropped and hence the handle must not be
    /// used afterwards.
    pub fn get_handle(&self) -> vk::SurfaceKHR {
        self.surface
    }
}

impl<'a, 'b> Drop for Surface<'a, 'b> {
    fn drop(&mut self) {
        unsafe {
            self.instance.get_khr_surface().unwrap().destroy_surface(self.surface, None);
        }
    }
}

assert_impl_all!(Surface: Send, Sync);