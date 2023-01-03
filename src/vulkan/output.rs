mod surface {
    use std::sync::Arc;
    use crate::output::OutputTarget;
    use crate::scene::CameraComponent;
    use crate::vulkan::AgnajiVulkan;

    pub enum SurfaceOutputCreateError {
        SurfaceUnsupported,
    }

    pub struct SurfaceOutput {
        agnaji: Arc<AgnajiVulkan>,

    }

    impl SurfaceOutput {
        /// If true the surface will always wait for a scene update before drawing the next frame.
        pub fn set_wait_for_scene_update(&self, wait: bool) {
            todo!()
        }
    }

    impl OutputTarget for SurfaceOutput {
        fn set_source_camera(&self, camera: Option<Arc<dyn CameraComponent>>) {
            todo!()
        }
    }
}

pub use surface::SurfaceOutputCreateError;
pub use surface::SurfaceOutput;