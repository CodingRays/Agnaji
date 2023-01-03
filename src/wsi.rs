use ash::vk;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
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
    ///
    /// The created surface must be destroyed by a call to
    /// [`VulkanSurfaceProvider::destroy_surface`] before the surface provider is dropped. Otherwise
    /// the surface provider must panic during drop.
    fn create_surface(&self, instance: &crate::vulkan::InstanceContext) -> Result<vk::SurfaceKHR, VulkanSurfaceCreateError>;

    /// Destroys the current surface.
    ///
    /// If no current surface exists this function panics.
    ///
    /// # Safety
    /// All derived vulkan objects of the surface must have been destroyed.
    unsafe fn destroy_surface(&self, instance: &crate::vulkan::InstanceContext);

    /// Returns the current surface.
    ///
    /// If no current surface exists this function panics.
    fn get_surface(&self) -> vk::SurfaceKHR;

    /// Returns the size of the canvas in pixels backing the surface (for example the window size)
    /// or [`None`] if that is currently undefined. If [`None`] is returned the renderer may not
    /// be able to create a swapchain so during normal use this function should return a valid size.
    fn get_canvas_size(&self) -> Option<CanvasSize>;
}