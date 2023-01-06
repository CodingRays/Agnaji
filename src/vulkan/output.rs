mod surface {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;
    use crate::output::OutputTarget;
    use crate::scene::CameraComponent;
    use crate::vulkan::AgnajiVulkan;
    use crate::vulkan::surface::VulkanSurfaceProvider;

    pub enum SurfaceOutputCreateError {
        SurfaceUnsupported,
    }

    pub struct SurfaceOutput {
        share: Arc<SurfaceOutputShare>,
        worker: Option<JoinHandle<()>>,
    }

    impl SurfaceOutput {
        pub(in crate::vulkan) fn new(agnaji: Arc<AgnajiVulkan>, surface_provider: Box<dyn VulkanSurfaceProvider>) -> Self {
            let share = Arc::new(SurfaceOutputShare::new(agnaji));

            let share_clone = share.clone();
            let worker = std::thread::spawn(move || {
                Self::run_worker(share_clone, surface_provider)
            });

            Self {
                share,
                worker: Some(worker)
            }
        }

        /// If true the surface will always wait for a scene update before drawing the next frame.
        pub fn set_wait_for_scene_update(&self, wait: bool) {
            self.share.wait_for_scene_update.store(wait, Ordering::SeqCst);
        }

        fn run_worker(share: Arc<SurfaceOutputShare>, surface_provider: Box<dyn VulkanSurfaceProvider>) {
            while !share.should_destroy() {
                if surface_provider.suspended() {
                    surface_provider.wait_unsuspended();
                }

                match surface_provider.create_surface(share.agnaji.instance.clone()) {
                    Ok(surface) => {
                        let _handle = surface.get_handle();

                        while !surface_provider.suspended() && !share.should_destroy() {
                            // todo do stuff with surface

                            std::thread::yield_now(); // Dont want to blow cpu usage during tests
                        }

                        drop(surface);
                    }
                    Err(err) => {
                        log::error!("Failed to create vulkan surface: {:?}", err);
                        std::thread::yield_now();
                    }
                }
            }
        }
    }

    impl OutputTarget for SurfaceOutput {
        fn set_source_camera(&self, camera: Option<Arc<dyn CameraComponent>>) {
            todo!()
        }
    }

    impl Drop for SurfaceOutput {
        fn drop(&mut self) {
            self.share.destroy.store(true, Ordering::SeqCst);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    struct SurfaceOutputShare {
        agnaji: Arc<AgnajiVulkan>,
        destroy: AtomicBool,

        wait_for_scene_update: AtomicBool,
    }

    impl SurfaceOutputShare {
        fn new(agnaji: Arc<AgnajiVulkan>) -> Self {
            Self {
                agnaji,
                destroy: AtomicBool::new(false),

                wait_for_scene_update: AtomicBool::new(true),
            }
        }

        fn should_destroy(&self) -> bool {
            self.destroy.load(Ordering::SeqCst)
        }
    }
}

pub use surface::SurfaceOutputCreateError;
pub use surface::SurfaceOutput;