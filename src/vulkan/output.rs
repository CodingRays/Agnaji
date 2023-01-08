mod surface {
    //! Output to a vulkan surface.
    //!
    //! The public api is the [`SurfaceOutput`] struct which implements the [`OutputTarget`] trait.
    //!
    //! Every [`SurfaceOutput`] spawns a new thread using [`SurfaceOutputWorker`] which will be
    //! managing the surface and render from it.

    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;

    use ash::vk;

    use crate::output::OutputTarget;
    use crate::scene::CameraComponent;
    use crate::vulkan::AgnajiVulkan;
    use crate::vulkan::device::DeviceProvider;
    use crate::vulkan::surface::VulkanSurfaceProvider;


    pub struct ColorSpaceFormats {
        color_space: vk::ColorSpaceKHR,
        formats: HashSet<vk::Format>,
    }

    pub type FormatSelectionFn = dyn Fn(&[ColorSpaceFormats]) -> (vk::ColorSpaceKHR, vk::Format) + Send;

    /// Output to a vulkan surface. The surface is provided by a [`VulkanSurfaceProvider`].
    ///
    /// By default this output will always wait for a scene update to start rendering a new frame.
    /// This behaviour can be controlled using [`SurfaceOutput::set_wait_for_scene_update`].
    pub struct SurfaceOutput {
        share: Arc<Share>,
        worker: Option<JoinHandle<()>>,
    }

    impl SurfaceOutput {
        /// Creates a new [`SurfaceOutput`].
        ///
        /// The `name` is a optional name that will be used for debugging and logging purposes only.
        pub(in crate::vulkan) fn new(agnaji: Arc<AgnajiVulkan>, surface_provider: Box<dyn VulkanSurfaceProvider>, name: Option<String>) -> Self {
            let share = Arc::new(Share::new(agnaji, name));

            let share_clone = share.clone();
            let worker = std::thread::spawn(move || {
                SurfaceOutputWorker::run(share_clone, surface_provider);
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
        ///
        /// **Note:** The format reselection will happen on a different thread and hence may be
        /// delayed quiet a bit from calling this function. In any case this function will not block.
        pub fn set_format_selection_fn(&self, selection_fn: Option<Box<FormatSelectionFn>>) {
            let mut guard = self.share.guarded.lock().unwrap();
            guard.format_selection_fn = selection_fn;
            guard.should_select_format = true;
        }

        /// Triggers a format reselection.
        ///
        /// **Note:** The format reselection will happen on a different thread and hence may be
        /// delayed quiet a bit from calling this function. In any case this function will not block.
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

    /// Shared struct between the [`SurfaceOutput`] instance and its associated
    /// [`SurfaceOutputWorker`] used for communication.
    struct Share {
        agnaji: Arc<AgnajiVulkan>,
        name: Option<String>,
        destroy: AtomicBool,

        guarded: Mutex<ShareGuarded>,
    }

    impl Share {
        fn new(agnaji: Arc<AgnajiVulkan>, name: Option<String>) -> Self {
            Self {
                agnaji,
                name,
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
        surface_provider: Box<dyn VulkanSurfaceProvider>,
    }

    impl SurfaceOutputWorker {
        fn run(share: Arc<Share>, surface_provider: Box<dyn VulkanSurfaceProvider>) {
            Self {
                share,
                surface_provider,
            }.run_internal();
        }

        fn run_internal(&self) {
            // How often did surface creation fail in a row. Used to determine wait times
            let mut err_repeat = 0;

            while !self.share.should_destroy() {
                let instance = self.share.agnaji.instance.clone();
                match unsafe { self.surface_provider.create_surface(&instance) } {
                    Ok(surface) => {
                        log::info!("Surface successfully created");
                        std::thread::yield_now();
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
                };
            }
        }

        fn run_surface_loop(&self, surface: vk::SurfaceKHR) {
            // todo check surface present support

            let supported_formats = match self.get_supported_surface_formats(surface) {
                Ok(v) => v,
                Err(err) => {
                    log::error!("Failed to query supported surface formats: {:?}", err);
                    return;
                }
            };

            while !self.share.should_destroy() {
                // todo do stuff with surface

                std::thread::yield_now(); // Dont want to blow cpu usage during tests
            }
        }

        /// Lists all supported surface formats for the provided surface.
        fn get_supported_surface_formats(&self, surface: vk::SurfaceKHR) -> Result<Box<[ColorSpaceFormats]>, vk::Result> {
            let device = &self.share.agnaji.device;
            let physical_device = device.get_physical_device();
            let instance = device.get_instance().get_instance();
            let khr_surface = device.get_instance().get_khr_surface().unwrap();

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

pub use surface::SurfaceOutput;