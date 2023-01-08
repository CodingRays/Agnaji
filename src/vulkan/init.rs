use std::collections::HashMap;
use std::sync::Arc;
use ash::vk;

use crate::vulkan::{AgnajiVulkan, InstanceContext, surface};
use crate::vulkan::device::MainDeviceReport;
use crate::vulkan::output::SurfaceOutput;
use crate::vulkan::surface::{SurfaceProviderId, VulkanSurfaceProvider};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DeviceReportGenerationError {
    SurfaceCreationFailed(vk::Result),
    Vulkan(vk::Result),
}

impl From<vk::Result> for DeviceReportGenerationError {
    fn from(error: vk::Result) -> Self {
        Self::Vulkan(error)
    }
}

/// Used to build a [`AgnajiVulkan`] instance.
pub struct AgnajiVulkanInitializer {
    instance: Arc<InstanceContext>,
    surfaces: Option<HashMap<SurfaceProviderId, RegisteredSurface>>,
}

impl AgnajiVulkanInitializer {
    /// Creates a new initializer. The vulkan instance is created as part of this function and as
    /// such any settings needed to configure the instance need to be passed to this function.
    ///
    /// In order to allow surface creation a list of required surface platforms need to be provided.
    /// If `surface_platforms` is empty or [`None`] the `VK_KHR_Surface` extension will also not be
    /// enabled.
    ///
    /// If `enable_debug` is false no debugging extensions or validation layers will be enabled and
    /// some engine systems may disable certain debugging tools. Otherwise debugging features will
    /// be enabled as supported by the current platform.
    pub fn new(surface_platforms: Option<&[surface::SurfacePlatform]>, enable_debug: bool) -> Self {
        let mut required_extensions = Vec::new();

        if let Some(surface_platforms) = surface_platforms {
            for surface_platform in surface_platforms {
                surface_platform.get_required_instance_extensions(&mut required_extensions);
            }
        }

        let entry = unsafe { ash::Entry::load() }.unwrap();
        let instance = Arc::new(InstanceContext::new(entry, enable_debug, required_extensions).unwrap());

        let surfaces = instance.get_khr_surface().map(|_| HashMap::new());

        AgnajiVulkanInitializer {
            instance,
            surfaces
        }
    }

    /// Equivalent to calling [`AgnajiVulkanInitializer::new`] with `surface_platforms`set to
    /// [`None`].
    pub fn new_headless(enable_debug: bool) -> Self {
        Self::new(None, enable_debug)
    }

    pub fn get_instance(&self) -> &Arc<InstanceContext> {
        &self.instance
    }

    /// Registers a surface provider use to check device support for surface presentation.
    ///
    /// If this initializer has been created with no surface support [`None`] is returned.
    ///
    /// An optional name can be provided which will be used for debugging and logging.
    pub fn register_surface(&mut self, surface_provider: Box<dyn VulkanSurfaceProvider>, name: Option<&str>) -> Option<SurfaceProviderId> {
        if let Some(surfaces) = self.surfaces.as_mut() {
            let id = SurfaceProviderId::new();
            let name = name.map(String::from);

            log::debug!("Registered vulkan surface provider {:?} with name {:?}", id, name);

            surfaces.insert(id, RegisteredSurface { name, surface_provider });

            Some(id)
        } else {
            None
        }
    }

    pub fn generate_device_reports(&mut self) -> Result<Box<[MainDeviceReport]>, DeviceReportGenerationError> {
        let physical_devices = unsafe { self.instance.get_instance().enumerate_physical_devices() }?;

        let mut reports = Vec::with_capacity(physical_devices.len());

        for physical_device in physical_devices {
            let queue_count = unsafe {
                self.instance.get_instance().get_physical_device_queue_family_properties2_len(physical_device)
            };

            let mut queue_surface_support: Box<[_]> = std::iter::repeat(true).take(queue_count).collect();

            // Yes were recreating every surface for every device but this doesnt need to be fast so its fine.
            // Properly supporting potential suspended errors is more important.
            if let Some(surfaces) = self.surfaces.as_ref() {
                let khr_surface = self.instance.get_khr_surface().unwrap();

                for (_, registered) in surfaces.iter() {
                    let surface = unsafe { registered.surface_provider.create_surface(&self.instance) }
                        .map_err(|err| DeviceReportGenerationError::SurfaceCreationFailed(err))?;

                    let handle = surface.get_handle();
                    for i in 0..queue_count {
                        if !unsafe { khr_surface.get_physical_device_surface_support(physical_device, i as u32, handle)? } {
                            queue_surface_support[i] = false;
                        }
                    }

                    drop(surface);
                }
            }

            reports.push(MainDeviceReport::generate_for(&self.instance, physical_device, &queue_surface_support)?);
        }

        Ok(reports.into_boxed_slice())
    }

    pub fn build(self, device: &MainDeviceReport) -> Option<(Arc<AgnajiVulkan>, Vec<(SurfaceProviderId, Arc<SurfaceOutput>)>)> {
        let device = Arc::new(device.create_device(self.instance.clone()).ok()?);

        if let Some(surfaces) = self.surfaces {
            let surfaces = surfaces.into_iter().map(|(id, registered)| (id, registered.surface_provider, registered.name));
            Some(AgnajiVulkan::new(self.instance, device, surfaces))
        } else {
            Some(AgnajiVulkan::new(self.instance, device, std::iter::empty()))
        }
    }
}

struct RegisteredSurface {
    name: Option<String>,
    surface_provider: Box<dyn VulkanSurfaceProvider>,
}