mod surface {
    //! Output to a vulkan surface.
    //!
    //! The public api is the [`SurfaceOutput`] struct which implements the [`OutputTarget`] trait.
    //!
    //! Every [`SurfaceOutput`] spawns a new thread using [`SurfaceOutputWorker`] which will be
    //! managing the surface and render from it.

    use std::collections::{HashMap, HashSet};
    use std::collections::hash_map::Keys;
    use std::iter::{Map, Repeat, Zip};
    use std::slice::Iter;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::JoinHandle;

    use ash::vk;
    use nalgebra::sup;

    use crate::output::OutputTarget;
    use crate::scene::CameraComponent;
    use crate::vulkan::AgnajiVulkan;
    use crate::vulkan::device::DeviceProvider;
    use crate::vulkan::surface::VulkanSurfaceProvider;

    /// Selects a format for a swapchain from the list of available formats.
    ///
    /// If this function returns [`None`] the default selection algorithm will be used as backup.
    pub type SurfaceFormatSelectionFn = dyn Fn(&SurfaceFormatList) -> Option<&SurfaceFormat> + Send;

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
        pub fn set_format_selection_fn(&self, selection_fn: Option<Box<SurfaceFormatSelectionFn>>) {
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
        format_selection_fn: Option<Box<SurfaceFormatSelectionFn>>,
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
            log::info!("Starting SurfaceOutput worker thread. (Output: {:?})", self.share.name);

            // How often did surface creation fail in a row. Used to determine wait times
            let mut err_repeat = 0;

            while !self.share.should_destroy() {
                let instance = self.share.agnaji.instance.clone();
                match unsafe { self.surface_provider.create_surface(&instance) } {
                    Ok(surface) => {
                        log::info!("Surface created (Output: {:?})", self.share.name);
                        if self.run_surface_loop(surface.get_handle()).is_ok() {
                            err_repeat = 0;
                        } else {
                            err_repeat += 1;
                            if err_repeat > 3 {
                                std::thread::sleep(std::time::Duration::from_millis(1000));
                            }
                        }
                    }
                    Err(err) => {
                        if err_repeat <= 2 {
                            log::error!("Failed to create vulkan surface: {:?} (Output: {:?})", err, self.share.name);
                            std::thread::yield_now();
                        } else {
                            let millis = std::cmp::min(2000, err_repeat * 10);
                            log::error!("Failed to create vulkan surface: {:?}. Retrying in {}ms. (Output: {:?})", err, millis, self.share.name);
                            std::thread::sleep(std::time::Duration::from_millis(millis));
                        }
                        err_repeat += 1;
                    }
                };
            }

            log::info!("SurfaceOutput worker thread destroyed. (Output: {:?})", self.share.name);
        }

        fn run_surface_loop(&self, surface: vk::SurfaceKHR) -> Result<(), vk::Result> {
            // todo check surface present support

            let supported_formats = self.get_supported_surface_formats(surface).map_err(|err| {
                log::error!("Failed to query supported surface formats: {:?}. (Output: {:?})", err, self.share.name);
                err
            })?;

            while !self.share.should_destroy() {
                // todo do stuff with surface

                std::thread::yield_now(); // Dont want to blow cpu usage during tests
            }

            Ok(())
        }

        /// Lists all supported surface formats for the provided surface.
        fn get_supported_surface_formats(&self, surface: vk::SurfaceKHR) -> Result<SurfaceFormatList, vk::Result> {
            let device = &self.share.agnaji.device;
            let physical_device = device.get_physical_device();
            let khr_surface = device.get_instance().get_khr_surface().unwrap();

            let supported_surface_formats = unsafe {
                khr_surface.get_physical_device_surface_formats(physical_device, surface)
            }?;

            Ok(SurfaceFormatList::from_surface_formats(supported_surface_formats.into_iter().map(|f| {
                SurfaceFormat {
                    color_space: f.color_space,
                    format: f.format,
                }
            })))
        }

        fn select_format<'a>(&self, supported: &'a SurfaceFormatList) -> &'a SurfaceFormat {
            let mut guard = self.share.guarded.lock().unwrap();
            guard.should_select_format = false;
            guard.format_selection_fn.as_ref().map(|f| (*f)(supported)).flatten()
                .or_else(|| Some(self.default_format_selection(supported))).unwrap()
        }

        /// The default format selection algorithm.
        ///
        /// Will select the highest quality format using at most 32bits per pixel from color spaces
        /// in the following order: SRGB_NONLINEAR, BT709_NONLINEAR, EXTENDED_SRGB_NONLINEAR, any
        /// other color space.
        ///
        /// If the above finds no format the first format in the provided list will be selected.
        fn default_format_selection<'a>(&self, supported: &'a SurfaceFormatList) -> &'a SurfaceFormat {
            const COLOR_SPACE_PRIORITIES: &[vk::ColorSpaceKHR] = &[
                vk::ColorSpaceKHR::SRGB_NONLINEAR,
                vk::ColorSpaceKHR::BT709_NONLINEAR_EXT,
                vk::ColorSpaceKHR::EXTENDED_SRGB_NONLINEAR_EXT,
            ];
            const FORMAT_PRIORITIES: &[vk::Format] = &[
                vk::Format::B10G11R11_UFLOAT_PACK32,
                vk::Format::A2R10G10B10_UNORM_PACK32,
                vk::Format::A2B10G10R10_UNORM_PACK32,
                vk::Format::E5B9G9R9_UFLOAT_PACK32,
                vk::Format::R8G8B8A8_SRGB,
                vk::Format::B8G8R8A8_SRGB,
                vk::Format::A8B8G8R8_SRGB_PACK32,
                vk::Format::R8G8B8_SRGB,
                vk::Format::B8G8R8_SRGB,
                vk::Format::R8G8B8A8_UNORM,
                vk::Format::B8G8R8A8_UNORM,
                vk::Format::A8B8G8R8_UNORM_PACK32,
                vk::Format::R8G8B8_UNORM,
                vk::Format::B8G8R8_UNORM,
                vk::Format::R5G5B5A1_UNORM_PACK16,
                vk::Format::B5G5R5A1_UNORM_PACK16,
                vk::Format::A1R5G5B5_UNORM_PACK16,
                vk::Format::R5G6B5_UNORM_PACK16,
                vk::Format::B5G6R5_UNORM_PACK16,
                vk::Format::R4G4B4A4_UNORM_PACK16,
                vk::Format::B4G4R4A4_UNORM_PACK16,
                vk::Format::A4R4G4B4_UNORM_PACK16,
                vk::Format::A4B4G4R4_UNORM_PACK16,
            ];
            for color_space in COLOR_SPACE_PRIORITIES {
                if let Some(formats) = supported.by_color_space(*color_space) {
                    let formats: HashMap<_, _> = formats.map(|f| (f.format, f)).collect();
                    for format in FORMAT_PRIORITIES {
                        if let Some(format) = formats.get(format) {
                            return format;
                        }
                    }
                }
            }

            for format in FORMAT_PRIORITIES {
                if let Some(mut color_spaces) = supported.by_format(*format) {
                    return color_spaces.next().unwrap();
                }
            }

            &supported.surface_formats()[0]
        }
    }

    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct SurfaceFormat {
        pub color_space: vk::ColorSpaceKHR,
        pub format: vk::Format,
    }

    pub struct SurfaceFormatList {
        surface_formats: Vec<SurfaceFormat>,
        by_color_space: HashMap<vk::ColorSpaceKHR, Vec<usize>>,
        by_format: HashMap<vk::Format, Vec<usize>>,
    }

    type ByIter<'a> = Map<Zip<Iter<'a, usize>, Repeat<&'a SurfaceFormatList>>, fn((&'a usize, &'a SurfaceFormatList)) -> &'a SurfaceFormat>;

    impl SurfaceFormatList {
        fn from_surface_formats<I>(surface_formats: I) -> Self where I: Iterator<Item=SurfaceFormat> {
            let surface_formats: Vec<_> = surface_formats.collect();

            let mut by_color_space: HashMap<vk::ColorSpaceKHR, Vec<usize>> = HashMap::new();
            let mut by_format: HashMap<vk::Format, Vec<usize>> = HashMap::new();

            for (index, SurfaceFormat { color_space, format }) in surface_formats.iter().enumerate() {
                if let Some(indices) = by_color_space.get_mut(color_space) {
                    indices.push(index);
                } else {
                    by_color_space.insert(*color_space, vec![index]);
                }

                if let Some(indices) = by_format.get_mut(format) {
                    indices.push(index);
                } else {
                    by_format.insert(*format, vec![index]);
                }
            }

            Self {
                surface_formats,
                by_color_space,
                by_format,
            }
        }

        pub fn has_color_space(&self, color_space: vk::ColorSpaceKHR) -> bool {
            self.by_color_space.contains_key(&color_space)
        }

        pub fn has_format(&self, format: vk::Format) -> bool {
            self.by_format.contains_key(&format)
        }

        pub fn has_surface_format(&self, color_space: vk::ColorSpaceKHR, format: vk::Format) -> bool {
            self.get_surface_format(color_space, format).is_some()
        }

        pub fn get_color_spaces<'a>(&'a self) -> Map<Keys<'_, vk::ColorSpaceKHR, Vec<usize>>, fn(&'a vk::ColorSpaceKHR) -> vk::ColorSpaceKHR> {
            self.by_color_space.keys().map(|v| *v)
        }

        pub fn get_formats<'a>(&'a self) -> Map<Keys<'_, vk::Format, Vec<usize>>, fn(&'a vk::Format) -> vk::Format> {
            self.by_format.keys().map(|v| *v)
        }

        pub fn get_surface_format(&self, color_space: vk::ColorSpaceKHR, format: vk::Format) -> Option<&SurfaceFormat> {
            self.by_color_space.get(&color_space).map(|indices| {
                for i in indices {
                    let surface_format = self.surface_formats.get(*i).unwrap();
                    if surface_format.format == format {
                        return Some(surface_format)
                    }
                }
                None
            }).flatten()
        }

        pub fn by_color_space(&self, color_space: vk::ColorSpaceKHR) -> Option<ByIter> {
            self.by_color_space.get(&color_space).map(|indices| {
                indices.iter()
                    .zip(std::iter::repeat(self))
                    .map(Self::get_from_index as for<'a> fn((&'a usize, &'a Self)) -> &'a SurfaceFormat)
            })
        }

        pub fn by_format(&self, format: vk::Format) -> Option<ByIter> {
            self.by_format.get(&format).map(|indices| {
                indices.iter()
                    .zip(std::iter::repeat(self))
                    .map(Self::get_from_index as for<'a> fn((&'a usize, &'a Self)) -> &'a SurfaceFormat)
            })
        }

        pub fn surface_formats(&self) -> &[SurfaceFormat] {
            &self.surface_formats
        }

        #[inline(always)]
        fn get_from_index<'a>(data: (&'a usize, &'a Self)) -> &'a SurfaceFormat {
            data.1.surface_formats.get(*data.0).unwrap()
        }
    }
}

pub use surface::SurfaceOutput;
pub use surface::SurfaceFormatSelectionFn;
pub use surface::SurfaceFormat;
pub use surface::SurfaceFormatList;