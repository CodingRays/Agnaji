pub mod device;
pub mod instance;
pub mod scene;
pub mod surface;
pub mod output;
mod swapchain;
pub mod init;

use std::sync::{Arc, Weak};

use crate::Agnaji;

pub use instance::InstanceContext;

use crate::scene::Scene;
use crate::vulkan::device::MainDeviceContext;
use crate::vulkan::output::SurfaceOutput;
use crate::vulkan::scene::VulkanScene;
use crate::vulkan::surface::{SurfaceProviderId, VulkanSurfaceProvider};

pub struct AgnajiVulkan {
    weak: Weak<Self>,
    instance: Arc<InstanceContext>,
    device: Arc<MainDeviceContext>,
}

impl AgnajiVulkan {
    fn new<T>(instance: Arc<InstanceContext>, device: Arc<MainDeviceContext>, surfaces: T) -> (Arc<Self>, Vec<(SurfaceProviderId, Arc<SurfaceOutput>)>)
        where T: Iterator<Item=(SurfaceProviderId, Box<dyn VulkanSurfaceProvider>, Option<String>)> {

        let agnaji = Arc::new_cyclic(|weak| {
            Self {
                weak: weak.clone(),
                instance,
                device
            }
        });

        let output = surfaces.map(|(id, surface, name)| {
            (id, Arc::new(SurfaceOutput::new(agnaji.clone(), surface, name)))
        }).collect::<Vec<_>>();

        (agnaji, output)
    }

    pub fn create_surface_output(&self, surface_provider: Box<dyn VulkanSurfaceProvider>, name: Option<String>) -> Result<Arc<SurfaceOutput>, ()> {
        Ok(Arc::new(SurfaceOutput::new(self.weak.upgrade().unwrap(), surface_provider, name)))
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