pub mod instance;
pub mod surface;

use std::sync::Arc;

use crate::Agnaji;

pub use instance::InstanceContext;

pub struct AgnajiVulkan {
    instance: Arc<InstanceContext>,
}

impl AgnajiVulkan {
    pub fn new(enable_debug: bool) -> Self {
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