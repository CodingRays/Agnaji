use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fmt::Formatter;

use ash::vk;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct APIVersion {
    version: u32,
}

impl APIVersion {
    pub const VERSION_1_0: Self = Self::from_raw(vk::API_VERSION_1_0);
    pub const VERSION_1_1: Self = Self::from_raw(vk::API_VERSION_1_1);
    pub const VERSION_1_2: Self = Self::from_raw(vk::API_VERSION_1_2);
    pub const VERSION_1_3: Self = Self::from_raw(vk::API_VERSION_1_3);

    /// Creates a new api version with variant set to 0
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            version: vk::make_api_version(0, major, minor, patch),
        }
    }

    /// Creates a new api version with the provided variant
    pub const fn new_with_variant(variant: u32, major: u32, minor: u32, patch: u32) -> Self {
        Self {
            version: vk::make_api_version(variant, major, minor, patch),
        }
    }

    /// Creates a new api version from the raw vulkan version value
    pub const fn from_raw(raw: u32) -> Self {
        Self {
            version: raw,
        }
    }

    /// Returns the variant of the version
    pub const fn get_variant(&self) -> u32 {
        vk::api_version_variant(self.version)
    }

    /// Returns the major version number
    pub const fn get_major(&self) -> u32 {
        vk::api_version_major(self.version)
    }

    /// Returns the minor version number
    pub const fn get_minor(&self) -> u32 {
        vk::api_version_minor(self.version)
    }

    /// Returns the patch number
    pub const fn get_patch(&self) -> u32 {
        vk::api_version_patch(self.version)
    }
}

impl std::fmt::Debug for APIVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "APIVersion({}.{}.{} [{}])",
            vk::api_version_major(self.version),
            vk::api_version_minor(self.version),
            vk::api_version_patch(self.version),
            vk::api_version_variant(self.version)
        ))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum InstanceCreateError {
    UnsupportedVersion(APIVersion),
    MissingRequiredExtensions(Vec<CString>),
    Vulkan(vk::Result)
}

impl From<vk::Result> for InstanceCreateError {
    fn from(result: vk::Result) -> Self {
        InstanceCreateError::Vulkan(result)
    }
}

pub struct InstanceContext {
    entry: ash::Entry,
    instance: ash::Instance,
    khr_surface: Option<ash::extensions::khr::Surface>,
    ext_debug_utils: Option<ash::extensions::ext::DebugUtils>,
    enabled_extensions: Box<[CString]>,
}

impl InstanceContext {
    pub fn new(entry: ash::Entry, enable_debug: bool, required_extensions: Vec<CString>) -> Result<Self, InstanceCreateError> {
        // Validate API version
        let version = match entry.try_enumerate_instance_version().map_err(|err| {
            log::error!("Failed to enumerate instance version {:?}", err);
            err
        })? {
            None => {
                log::error!("Vulkan instance version is 1.0 which is unsupported");
                return Err(InstanceCreateError::UnsupportedVersion(APIVersion::VERSION_1_0));
            },
            Some(version) => APIVersion::from_raw(version),
        };

        if version.get_variant() != 0 {
            log::error!("Vulkan instance has variant != 0 which is unsupported");
            return Err(InstanceCreateError::UnsupportedVersion(version));
        }

        if version.get_major() != 1 {
            log::error!("Vulkan instance has major version != 1 which is unsupported");
            return Err(InstanceCreateError::UnsupportedVersion(version));
        }

        // Check extension support
        let supported_extensions: HashSet<_> = entry.enumerate_instance_extension_properties(None).map_err(|err| {
            log::error!("Failed to enumerate instance extension properties: {:?}", err);
            err
        })?.into_iter().map(|e| CString::from(unsafe { CStr::from_ptr(e.extension_name.as_ptr()) } )).collect();

        let mut enabled_extensions = HashSet::new();

        if supported_extensions.contains(ash::extensions::ext::DebugUtils::name()) && enable_debug {
            enabled_extensions.insert(CString::from(ash::extensions::ext::DebugUtils::name()));
        }

        let khr_portability_enumeration_name = CString::from(CStr::from_bytes_with_nul(b"VK_KHR_portability_enumeration\0").unwrap());
        if supported_extensions.contains(&khr_portability_enumeration_name) {
            enabled_extensions.insert(khr_portability_enumeration_name);
        }

        let mut missing_extensions = Vec::new();
        for required_extension in required_extensions.into_iter() {
            if supported_extensions.contains(&required_extension) {
                enabled_extensions.insert(required_extension);
            } else {
                missing_extensions.push(required_extension);
            }
        }
        if !missing_extensions.is_empty() {
            return Err(InstanceCreateError::MissingRequiredExtensions(missing_extensions));
        }

        let khr_surface_enabled = enabled_extensions.contains(ash::extensions::khr::Surface::name());
        let ext_debug_utils = enabled_extensions.contains(ash::extensions::ext::DebugUtils::name());
        let enabled_extensions: Box<[_]> = enabled_extensions.into_iter().collect();
        let enabled_extensions_ptr: Vec<_> = enabled_extensions.iter().map(|e| e.as_ptr()).collect();

        // Check layer support
        let mut enabled_layers = Vec::new();
        if enable_debug {
            let supported_layers: HashSet<_> = entry.enumerate_instance_layer_properties().map_err(|err| {
                log::error!("Failed to enumerate instance layer properties: {:?}", err);
                err
            })?.into_iter().map(|e| CString::from(unsafe { CStr::from_ptr(e.layer_name.as_ptr()) } )).collect();

            let khronos_validation_name = CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0").unwrap();
            if supported_layers.contains(khronos_validation_name) {
                enabled_layers.push(khronos_validation_name);
            } else {
                log::warn!("Debugging is enabled but VK_LAYER_KHRONOS_validation is not supported by instance");
            }
        }
        let enabled_layers_ptr: Vec<_> = enabled_layers.iter().map(|l| l.as_ptr()).collect();

        // Create vulkan instance
        let mut instance_create_info = vk::InstanceCreateInfo::builder()
            .enabled_layer_names(&enabled_layers_ptr)
            .enabled_extension_names(&enabled_extensions_ptr);

        let mut messenger_create_info;
        if ext_debug_utils {
            messenger_create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::INFO | vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE)
                .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION)
                .pfn_user_callback(Some(debug_log_callback));

            instance_create_info = instance_create_info.push_next(&mut messenger_create_info);
        }

        log::info!("Creating vulkan instance {:?} Enabled extensions: {:?} Enabled layers: {:?}", version, enabled_extensions, enabled_layers);

        let instance = unsafe {
            entry.create_instance(&instance_create_info, None)
        }.map_err(|err| {
            log::error!("Failed to create vulkan instance: {:?}", err);
            err
        })?;

        let khr_surface = if khr_surface_enabled {
            Some(ash::extensions::khr::Surface::new(&entry, &instance))
        } else {
            None
        };
        let ext_debug_utils = if ext_debug_utils {
            Some(ash::extensions::ext::DebugUtils::new(&entry, &instance))
        } else {
            None
        };

        Ok(Self {
            entry,
            instance,
            khr_surface,
            ext_debug_utils,
            enabled_extensions,
        })
    }

    pub fn get_entry(&self) -> &ash::Entry {
        &self.entry
    }

    pub fn get_instance(&self) -> &ash::Instance {
        &self.instance
    }

    pub fn get_khr_surface(&self) -> Option<&ash::extensions::khr::Surface> {
        self.khr_surface.as_ref()
    }

    pub fn get_ext_debug_utils(&self) -> Option<&ash::extensions::ext::DebugUtils> {
        self.ext_debug_utils.as_ref()
    }

    pub fn is_extension_enabled(&self, name: &CStr) -> bool {
        for ext in self.enabled_extensions.iter() {
            if ext.as_c_str() == name {
                return true;
            }
        }

        false
    }
}

impl Drop for InstanceContext {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

unsafe extern "system" fn debug_log_callback(message_severity: vk::DebugUtilsMessageSeverityFlagsEXT, _message_types: vk::DebugUtilsMessageTypeFlagsEXT, p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT, _p_user_data: *mut std::ffi::c_void) -> vk::Bool32 {
    if let Err(_) = std::panic::catch_unwind(|| {
        match unsafe { CStr::from_ptr((*p_callback_data).p_message) }.to_str() {
            Ok(message) => {
                match message_severity {
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => {
                        log::error!(target: "agnaji::vulkan_debug", "{}", message);
                    },
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => {
                        log::warn!(target: "agnaji::vulkan_debug", "{}", message);
                    },
                    vk::DebugUtilsMessageSeverityFlagsEXT::INFO => {
                        log::info!(target: "agnaji::vulkan_debug", "{}", message);
                    },
                    vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => {
                        log::debug!(target: "agnaji::vulkan_debug", "{}", message);
                    },
                    _ => {
                        log::warn!("Unknown debug utils message severity: {:?}; {}", message_severity, message);
                    }
                }
            },
            Err(err) => {
                log::error!("Debug utils messenger received invalid message: {:?}", err);
            }
        };
    }) {
        log::error!("Panic in debug utils messenger callback! Aborting...");
        std::process::exit(1);
    }

    vk::FALSE
}