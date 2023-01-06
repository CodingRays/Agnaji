pub mod device;
pub mod instance;
pub mod scene;
pub mod surface;
mod output;

use std::sync::{Arc, Weak};

use crate::Agnaji;

pub use instance::InstanceContext;

use crate::scene::Scene;
use crate::vulkan::device::{MainDeviceContext, MainDeviceReport};
use crate::vulkan::output::SurfaceOutput;
use crate::vulkan::scene::VulkanScene;

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

    pub fn generate_main_device_report(&self) -> Box<[MainDeviceReport]> {
        let physical_devices = unsafe { self.instance.get_instance().enumerate_physical_devices().unwrap() };

        let mut device_reports = Vec::with_capacity(physical_devices.len());
        for physical_device in physical_devices {
            device_reports.push(MainDeviceReport::generate_for(&self.instance, physical_device).unwrap());
        }

        device_reports.into_boxed_slice()
    }

    pub fn set_main_device(&self, device: &MainDeviceReport) {
        let main_device = device.create_device(self.instance.clone()).unwrap();
    }

    pub fn create_surface_output(&self) -> Result<Arc<SurfaceOutput>, ()> {
        todo!()
    }

    /// Creates a new scene. See [`Agnaji::create_scene`] for more details.
    ///
    /// This function is called internally when [`Agnaji::create_scene`] is called and is only
    /// provided so that any caller doesnt have to cast the returned [`Scene`] if they need access
    /// to the underlying [`VulkanScene`].
    pub fn create_vulkan_scene(&self) -> Arc<VulkanScene> {
        todo!()
    }
}

impl Agnaji for AgnajiVulkan {
    fn create_scene(&self) -> Arc<dyn Scene> {
        self.create_vulkan_scene()
    }
}