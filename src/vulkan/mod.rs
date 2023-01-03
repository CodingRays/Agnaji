pub mod instance;
pub mod surface;
mod output;

use std::sync::{Arc, Weak};

use crate::Agnaji;

pub use instance::InstanceContext;
use crate::scene::Scene;
use crate::vulkan::output::SurfaceOutput;

pub struct AgnajiVulkan {
    weak: Weak<Self>,
    instance: Arc<InstanceContext>,
}

impl AgnajiVulkan {
    /// Creates a new render backend supporting the selected surface platforms.
    ///
    /// If `surface_platforms` is empty this is equivalent to calling [`AgnajiVulkan::new_headless`].
    pub fn new(enable_debug: bool, surface_platforms: &[surface::SurfacePlatform]) -> Arc<Self> {
        if surface_platforms.is_empty() {
            Self::new_headless(enable_debug)
        } else {
            let entry = unsafe { ash::Entry::load() }.unwrap();

            let mut extensions = Vec::new();
            for surface_platform in surface_platforms {
                surface_platform.get_required_instance_extensions(&mut extensions);
            }

            let instance = Arc::new(InstanceContext::new(entry, enable_debug, extensions).unwrap());

            Self::init_from_instance(instance)
        }
    }

    /// Creates a new render backed without any surface support
    pub fn new_headless(enable_debug: bool) -> Arc<Self> {
        let entry = unsafe { ash::Entry::load() }.unwrap();
        let instance = Arc::new(InstanceContext::new(entry, enable_debug, Vec::new()).unwrap());

        Self::init_from_instance(instance)
    }

    fn init_from_instance(instance: Arc<InstanceContext>) -> Arc<Self> {
        Arc::new_cyclic(|weak| {
            Self {
                weak: weak.clone(),
                instance,
            }
        })
    }

    pub fn generate_main_device_report(&self) {
    }

    pub fn create_surface_output(&self) -> Result<Arc<SurfaceOutput>, ()> {
        todo!()
    }
}

impl Agnaji for AgnajiVulkan {
    fn create_scene(&self) -> Arc<dyn Scene> {
        todo!()
    }
}