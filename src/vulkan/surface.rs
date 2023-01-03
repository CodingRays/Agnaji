use std::ffi::CString;
use std::sync::Arc;
use static_assertions::assert_impl_all;

use ash::vk;

use crate::wsi::*;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum NativePlatform {
    Windows,
    Wayland,
    Xlib,
    Xcb,
    Android,
    Metal,
    Headless,
}

impl NativePlatform {
    pub fn required_instance_extensions(&self, extensions: &mut Vec<CString>) {
        extensions.push(CString::from(ash::extensions::khr::Surface::name()));
        match self {
            NativePlatform::Windows => extensions.push(CString::from(ash::extensions::khr::Win32Surface::name())),
            NativePlatform::Wayland => extensions.push(CString::from(ash::extensions::khr::WaylandSurface::name())),
            NativePlatform::Xlib => extensions.push(CString::from(ash::extensions::khr::XlibSurface::name())),
            NativePlatform::Xcb => extensions.push(CString::from(ash::extensions::khr::XcbSurface::name())),
            NativePlatform::Android => extensions.push(CString::from(ash::extensions::khr::AndroidSurface::name())),
            NativePlatform::Metal => extensions.push(CString::from(ash::extensions::ext::MetalSurface::name())),
            NativePlatform::Headless => extensions.push(CString::from(ash::extensions::ext::HeadlessSurface::name())),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum VulkanSurfaceCreateError {
    /// The surface provider is currently suspended.
    Suspended,

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
    /// Any surface must be destroyed before calling this function otherwise this function panics.
    fn wait_unsuspended(&self);

    /// Creates a new surface.
    ///
    /// If a surface already exists this function panics.
    fn create_surface(&self, instance: Arc<crate::vulkan::InstanceContext>) -> Result<Surface, VulkanSurfaceCreateError>;

    /// Returns the size of the canvas in pixels backing the surface (for example the window size)
    /// or [`None`] if that is currently undefined. If [`None`] is returned the renderer may not
    /// be able to create a swapchain so during normal use this function should return a valid size.
    fn get_canvas_size(&self) -> Option<CanvasSize>;
}

/// Safe wrapper to allow a [`Send`] only [`FnOnce`] that is called on drop to be used in a [`Sync`]
/// struct.
struct DropFnWrapper<'a> {
    drop_fn: Option<Box<dyn FnOnce() + Send + 'a>>,
}

impl<'a> DropFnWrapper<'a> {
    fn new<F>(drop_fn: F)-> Self where F: FnOnce() + Send + 'a {
        Self {
            drop_fn: Some(Box::new(drop_fn))
        }
    }
}

impl<'a> Drop for DropFnWrapper<'a> {
    fn drop(&mut self) {
        self.drop_fn.take().unwrap()();
    }
}

// Safe because the drop fn is only ever called during drop
unsafe impl<'a> Sync for DropFnWrapper<'a> {
}

/// Wrapper of a vulkan surface.
///
/// Automatically destroys the surface when this struct is dropped.
pub struct Surface<'a> {
    instance: Arc<crate::vulkan::InstanceContext>,
    surface: vk::SurfaceKHR,

    #[allow(unused)]
    drop_fn: DropFnWrapper<'a>,
}

impl<'a> Surface<'a> {
    /// Creates a new instance of this struct for the provided surface.
    ///
    /// The `drop_fn` is called after the surface has been destroyed.
    pub fn new<F>(instance: Arc<crate::vulkan::InstanceContext>, surface: vk::SurfaceKHR, drop_fn: F) -> Self where F: FnOnce() + Send + 'a  {
        let drop_fn = DropFnWrapper::new(drop_fn);

        if instance.get_khr_surface().is_none() {
            panic!("Called Surface::new with instance that does not have the VK_KHR_surface extension enabled");
        }
        if surface == vk::SurfaceKHR::null() {
            panic!("Called Surface::new with null surface");
        }

        Self {
            instance,
            surface,
            drop_fn,
        }
    }
}

impl<'a> Drop for Surface<'a> {
    fn drop(&mut self) {
        unsafe {
            self.instance.get_khr_surface().unwrap().destroy_surface(self.surface, None);
        }
    }
}

assert_impl_all!(Surface: Send, Sync);