mod surface {
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;

    use ash::vk;

    use crate::output::OutputTarget;
    use crate::scene::CameraComponent;
    use crate::vulkan::AgnajiVulkan;
    use crate::vulkan::device::{DeviceProvider, SwapchainProvider};
    use crate::vulkan::surface::VulkanSurfaceProvider;


    pub struct ColorSpaceFormats {
        color_space: vk::ColorSpaceKHR,
        formats: HashSet<vk::Format>,
    }

    pub enum SurfaceOutputCreateError {
        SurfaceUnsupported,
    }

    pub type FormatSelectionFn = dyn Fn(&[ColorSpaceFormats]) -> (vk::ColorSpaceKHR, vk::Format) + Send;

    pub struct SurfaceOutput {
        share: Arc<Share>,
        worker: Option<JoinHandle<()>>,
    }

    impl SurfaceOutput {
        pub(in crate::vulkan) fn new(agnaji: Arc<AgnajiVulkan>, surface_provider: Box<dyn VulkanSurfaceProvider>) -> Self {
            let share = Arc::new(Share::new(agnaji));

            let share_clone = share.clone();
            let worker = std::thread::spawn(move || {
                todo!()
            });

            Self {
                share,
                worker: Some(worker)
            }
        }

        /// If true the surface will always wait for a scene update before drawing the next frame.
        pub fn set_wait_for_scene_update(&self, wait: bool) {
            self.share.guarded.lock().unwrap().wait_for_scene_update = wait;
        }

        /// Sets the format selection function. If [`None`] the default format selection will be
        /// used.
        ///
        /// Automatically triggers a format reselection even if the same selection function is
        /// provided. If only reselection is needed [`SurfaceOutput::reselect_format`] should be
        /// called instead.
        pub fn set_format_selection_fn(&self, selection_fn: Option<Box<FormatSelectionFn>>) {
            let mut guard = self.share.guarded.lock().unwrap();
            guard.format_selection_fn = selection_fn;
            guard.should_select_format = true;
        }

        pub fn reselect_format(&self) {
            self.share.guarded.lock().unwrap().should_select_format = true;
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

    struct Share {
        agnaji: Arc<AgnajiVulkan>,
        destroy: AtomicBool,

        guarded: Mutex<ShareGuarded>,
    }

    impl Share {
        fn new(agnaji: Arc<AgnajiVulkan>) -> Self {
            Self {
                agnaji,
                destroy: AtomicBool::new(false),

                guarded: Mutex::new(ShareGuarded {
                    format_selection_fn: None,
                    should_select_format: false,

                    wait_for_scene_update: true,
                })
            }
        }

        fn should_destroy(&self) -> bool {
            self.destroy.load(Ordering::SeqCst)
        }
    }

    struct ShareGuarded {
        format_selection_fn: Option<Box<FormatSelectionFn>>,
        should_select_format: bool,

        wait_for_scene_update: bool,
    }

    struct SurfaceOutputWorker {
        share: Arc<Share>,
        device: Arc<dyn SwapchainProvider>,
        surface_provider: Box<dyn VulkanSurfaceProvider>,
    }

    impl SurfaceOutputWorker {
        fn run(share: Arc<Share>, device: Arc<dyn SwapchainProvider>, surface_provider: Box<dyn VulkanSurfaceProvider>) {
            Self {
                share,
                device,
                surface_provider,
            }.run_internal();
        }

        fn run_internal(&self) {
            // How often did surface creation fail in a row. Used to determine wait times
            let mut err_repeat = 0;

            while !self.share.should_destroy() {
                while self.surface_provider.suspended() {
                    self.surface_provider.wait_unsuspended();
                    err_repeat = 0;
                }

                match self.surface_provider.create_surface(self.share.agnaji.instance.clone()) {
                    Ok(surface) => {
                        err_repeat = 0;
                        self.run_surface_loop(surface.get_handle());
                        drop(surface);
                    }
                    Err(err) => {
                        if err_repeat <= 2 {
                            log::error!("Failed to create vulkan surface: {:?}", err);
                            std::thread::yield_now();
                        } else {
                            let millis = std::cmp::min(2000, err_repeat * 10);
                            log::error!("Failed to create vulkan surface: {:?}. Retrying in {}ms", err, millis);
                            std::thread::sleep(std::time::Duration::from_millis(millis));
                        }
                        err_repeat += 1;
                    }
                }
            }
        }

        fn run_surface_loop(&self, surface: vk::SurfaceKHR) {
            let supported_formats = match self.get_supported_surface_formats(surface) {
                Ok(v) => v,
                Err(err) => {
                    log::error!("Failed to query supported surface formats: {:?}", err);
                    return;
                }
            };

            while !self.surface_provider.suspended() && !self.share.should_destroy() {
                // todo do stuff with surface

                std::thread::yield_now(); // Dont want to blow cpu usage during tests
            }
        }

        fn get_supported_surface_formats(&self, surface: vk::SurfaceKHR) -> Result<Box<[ColorSpaceFormats]>, vk::Result> {
            let physical_device = self.device.get_physical_device();
            let instance = self.device.get_instance().get_instance();
            let khr_surface = self.device.get_instance().get_khr_surface().unwrap();

            let supported_formats = unsafe {
                khr_surface.get_physical_device_surface_formats(physical_device, surface)
            }?;

            let mut color_attachment_formats: HashSet<_> = supported_formats.iter().map(|f| f.format).collect();
            color_attachment_formats.retain(|f| {
                let format_properties = unsafe {
                    instance.get_physical_device_format_properties(physical_device, *f)
                };
                format_properties.optimal_tiling_features.contains(vk::FormatFeatureFlags::COLOR_ATTACHMENT)
            });

            Ok(supported_formats.into_iter()
                .filter(|f| color_attachment_formats.contains(&f.format))
                .fold(HashMap::<vk::ColorSpaceKHR, HashSet<vk::Format>>::new(), |mut map, f| {
                    if let Some(formats) = map.get_mut(&f.color_space) {
                        formats.insert(f.format);
                    } else {
                        let mut formats = HashSet::new();
                        formats.insert(f.format);
                        map.insert(f.color_space, formats);
                    }
                    map
                }).into_iter()
                .map(|(color_space, formats)| ColorSpaceFormats { color_space, formats })
                .collect())
        }
    }
}

pub use surface::SurfaceOutputCreateError;
pub use surface::SurfaceOutput;