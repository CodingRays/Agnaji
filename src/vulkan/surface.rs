use std::any::Any;
use std::ffi::CString;
use std::sync::Arc;

use ash::vk;
use static_assertions::assert_impl_all;
use crate::utils::define_counting_id_type;

use crate::prelude::*;

define_counting_id_type!(pub, SurfaceProviderId);

/// Describes a native platform used to create surfaces
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SurfacePlatform {
    Android,
    Headless,
    Metal,
    Wayland,
    Windows,
    Xcb,
    Xlib,
}

impl SurfacePlatform {
    /// Stores all required instance extensions to use this surface platform in the provided Vec.
    pub fn get_required_instance_extensions(&self, extensions: &mut Vec<CString>) {
        extensions.push(CString::from(ash::extensions::khr::Surface::name()));
        match self {
            SurfacePlatform::Android => extensions.push(CString::from(ash::extensions::khr::AndroidSurface::name())),
            SurfacePlatform::Headless => extensions.push(CString::from(ash::extensions::ext::HeadlessSurface::name())),
            SurfacePlatform::Metal => extensions.push(CString::from(ash::extensions::ext::MetalSurface::name())),
            SurfacePlatform::Wayland => extensions.push(CString::from(ash::extensions::khr::WaylandSurface::name())),
            SurfacePlatform::Windows => extensions.push(CString::from(ash::extensions::khr::Win32Surface::name())),
            SurfacePlatform::Xcb => extensions.push(CString::from(ash::extensions::khr::XcbSurface::name())),
            SurfacePlatform::Xlib => extensions.push(CString::from(ash::extensions::khr::XlibSurface::name())),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum VulkanSurfaceCreateError {
    /// The surface provider is currently suspended.
    Suspended,

    /// Tried to create a surface when one already exists.
    SurfaceAlreadyExists,

    /// A vulkan function did not return [`vk::Result::SUCCESS`]
    VulkanError(vk::Result),
}

/// Provides a api to create and use vulkan surfaces associated with some canvas (for example a
/// window).
///
/// The surface provider has functions to create and destroy the vulkan surface. On some platforms
/// a surface may need to be destroyed for external reasons. To allow for this any calling code
/// must periodically call [`VulkanSurfaceProvider::suspended`] to check if this is needed.
///
/// Lifetime of the surface is managed by the code using the surface provider. As such any misuse
/// of functions is an indication that the calling code has failed in some way and should be handled
/// by a panic.
pub trait VulkanSurfaceProvider: Send {

    /// If this function returns true any surface must be destroyed as soon as possible and
    /// attempting to create a new surface will fail with [`VulkanSurfaceCreateError::Suspended`].
    ///
    /// # Important
    /// Other external systems may be blocked until the surface has been destroyed so any code using
    /// the surface must always be able to call this function and destroy the surface without
    /// waiting on external systems.
    fn suspended(&self) -> bool;

    /// Blocks and waits for the surface provider to become unsuspended.
    ///
    /// # Panics
    /// Any surface must be destroyed before calling this function otherwise this function panics.
    fn wait_unsuspended(&self);

    /// Creates a new surface.
    ///
    /// # Panics
    /// If a surface already exists this function panics.
    ///
    /// # Safety
    /// The returned function *must* be called after the surface has been destroyed.
    fn create_surface<'a, 'b>(&'a self, instance: &'b crate::vulkan::InstanceContext) -> Result<Surface<'a, 'b>, VulkanSurfaceCreateError>;

    /// Returns the size of the canvas in pixels backing the surface (for example the window size)
    /// or [`None`] if that is currently undefined. If [`None`] is returned the renderer may not
    /// be able to create a swapchain so during normal use this function should return a valid size.
    fn get_canvas_size(&self) -> Option<Vec2u32>;
}

/// Marker trait for objects which will be passed as a guard to a [`Surface`] instance.
pub trait SurfaceGuard: Send + Sync {
}

/// Wrapper of a vulkan surface.
///
/// Automatically destroys the surface when this struct is dropped.
pub struct Surface<'a, 'b> {
    instance: &'b crate::vulkan::InstanceContext,
    surface: vk::SurfaceKHR,

    #[allow(unused)]
    guard: Box<dyn SurfaceGuard + 'a>,
}

impl<'a, 'b> Surface<'a, 'b> {
    /// Creates a new instance of this struct for the provided surface.
    ///
    /// The `guard` is dropped after the surface has been destroyed. This can be used to keep track
    /// of surface state.
    pub fn new(instance: &'b crate::vulkan::InstanceContext, surface: vk::SurfaceKHR, guard: Box<dyn SurfaceGuard + 'a>) -> Self {
        if instance.get_khr_surface().is_none() {
            panic!("Called Surface::new with instance that does not have the VK_KHR_surface extension enabled");
        }
        if surface == vk::SurfaceKHR::null() {
            panic!("Called Surface::new with null surface");
        }

        Self {
            instance,
            surface,
            guard,
        }
    }

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