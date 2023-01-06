use std::sync::Arc;

use crate::vulkan::device::SwapchainProvider;
use crate::vulkan::surface::Surface;

pub struct SwapchainSupport {

}

pub struct Swapchain<'a, 'b: 'a> {
    surface: &'a Surface<'b>,
}

impl<'a, 'b: 'a> Swapchain<'a, 'b> {
    pub(in crate::vulkan) fn new(device: Arc<dyn SwapchainProvider>, surface: &'a Surface<'b>) -> Self {
        todo!()
    }
}