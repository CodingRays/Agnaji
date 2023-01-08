use std::ffi::{CStr, CString};
use std::panic::UnwindSafe;
use std::sync::Arc;
use raw_window_handle::HasRawDisplayHandle;
use agnaji::vulkan::AgnajiVulkan;
use agnaji::vulkan::init::AgnajiVulkanInitializer;
use agnaji::vulkan::output::SurfaceOutput;
use agnaji::winit::{Window, WinitBackend};

pub fn run_with_window<F>(name: &str, f: F) where F: FnOnce(Arc<WinitBackend>, Arc<Window>, Arc<SurfaceOutput>, Arc<AgnajiVulkan>) + Send + UnwindSafe + 'static {
    pretty_env_logger::init();

    let name = name.to_string();
    agnaji::winit::run(move |backend| {
        let window = backend.create_window(name, None).unwrap();
        let surface_provider = window.as_vulkan_surface_provider();

        let mut required_extensions = Vec::new();
        for ext in ash_window::enumerate_required_extensions(window.get_window().raw_display_handle()).unwrap() {
            required_extensions.push(CString::from(unsafe { CStr::from_ptr(*ext) }));
        }

        let mut initializer = AgnajiVulkanInitializer::new(required_extensions.into_iter(), true);
        initializer.register_surface(surface_provider, Some("main")).unwrap();

        let devices = initializer.generate_device_reports().unwrap();
        let mut selected = None;
        for device in devices.iter() {
            if device.is_suitable() {
                selected = Some(device);
            }
        }

        if let Some(selected) = selected {
            let (agnaji, mut surfaces) = initializer.build(selected).unwrap();
            let surface = surfaces.remove(0).1;

            f(backend, window, surface, agnaji);
        } else {
            log::error!("Failed to find suitable device");
        }
    })
}