pub mod instance;
pub mod surface;

use std::sync::Arc;

use crate::Agnaji;

pub use instance::InstanceContext;

pub struct AgnajiVulkan {
    instance: Arc<InstanceContext>,
}

impl AgnajiVulkan {
    /// Creates a new render backend supporting the selected surface platforms.
    ///
    /// If `native_platforms` is empty this is equivalent to calling [`AgnajiVulkan::new_headless`].
    pub fn new(enable_debug: bool, native_platforms: &[surface::NativePlatform]) -> Self {
        if native_platforms.is_empty() {
            Self::new_headless(enable_debug)
        } else {
            let entry = unsafe { ash::Entry::load() }.unwrap();

            let mut extensions = Vec::new();
            for native_platform in native_platforms {
                native_platform.required_instance_extensions(&mut extensions);
            }

            let instance = Arc::new(InstanceContext::new(entry, enable_debug, extensions).unwrap());

            Self {
                instance
            }
        }
    }

    /// Creates a new render backed without any surface support
    pub fn new_headless(enable_debug: bool) -> Self {
        let entry = unsafe { ash::Entry::load() }.unwrap();
        let instance = Arc::new(InstanceContext::new(entry, enable_debug, Vec::new()).unwrap());

        Self {
            instance,
        }
    }

    pub fn generate_device_report(&self) {
    }
}

impl Agnaji for AgnajiVulkan {

}