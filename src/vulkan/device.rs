use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fmt::Formatter;
use std::sync::{Arc, Mutex, MutexGuard};

use ash::vk;

use crate::vulkan::device::DeviceCreateError::Vulkan;
use crate::vulkan::instance::APIVersion;

use crate::vulkan::InstanceContext;

pub trait DeviceProvider {
    fn get_instance(&self) -> &InstanceContext;

    fn get_physical_device(&self) -> vk::PhysicalDevice;

    fn get_device(&self) -> &ash::Device;
}

pub trait SwapchainProvider: DeviceProvider {
    fn get_swapchain_khr(&self) -> Option<&ash::extensions::khr::Swapchain>;
}

pub struct DeviceQueue {
    queue: Mutex<vk::Queue>,
    queue_family: u32,
}

impl DeviceQueue {
    fn new(queue: vk::Queue, family: u32) -> Self {
        Self {
            queue: Mutex::new(queue),
            queue_family: family,
        }
    }

    pub fn lock(&self) -> Option<MutexGuard<vk::Queue>> {
        self.queue.lock().ok()
    }

    pub fn get_queue_family(&self) -> u32 {
        self.queue_family
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DeviceCreateError {
    NotSupported,
    Vulkan(vk::Result),
}

impl From<vk::Result> for DeviceCreateError {
    fn from(err: vk::Result) -> Self {
        Vulkan(err)
    }
}

pub struct MainDeviceContext {
    instance: Arc<InstanceContext>,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    khr_buffer_device_address: ash::extensions::khr::BufferDeviceAddress,
    khr_synchronization_2: ash::extensions::khr::Synchronization2,
    khr_timeline_semaphore: ash::extensions::khr::TimelineSemaphore,
    khr_maintenance_4: Option<ash::extensions::khr::Maintenance4>,
    khr_swapchain: Option<ash::extensions::khr::Swapchain>,
    enabled_extensions: HashSet<CString>,
    main_queue: DeviceQueue,
    compute_queue: Option<DeviceQueue>,
    transfer_queue: Option<DeviceQueue>,
}

impl DeviceProvider for MainDeviceContext {
    fn get_instance(&self) -> &InstanceContext {
        &self.instance
    }

    fn get_physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }

    fn get_device(&self) -> &ash::Device {
        &self.device
    }
}

impl SwapchainProvider for MainDeviceContext {
    fn get_swapchain_khr(&self) -> Option<&ash::extensions::khr::Swapchain> {
        self.khr_swapchain.as_ref()
    }
}

pub struct MainDeviceReport {
    name: String,
    api_version: APIVersion,
    uuid: [u8; vk::UUID_SIZE],
    physical_device: vk::PhysicalDevice,
    config: Option<MainDeviceConfig>,
    warnings: Box<[String]>,
    errors: Box<[String]>,
}

impl MainDeviceReport {
    pub fn generate_for(instance: &InstanceContext, physical_device: vk::PhysicalDevice) -> Result<Self, vk::Result> {
        let khr_surface = instance.get_khr_surface();
        let instance = instance.get_instance();

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        let properties = unsafe {
            instance.get_physical_device_properties(physical_device)
        };

        let name = String::from(unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_str().unwrap());

        let api_version = APIVersion::from_raw(properties.api_version);
        if api_version.get_variant() != 0 {
            errors.push(String::from("Device API variant is not 0"));
        }
        if api_version.get_major() != 1 {
            errors.push(String::from("Device API major version is not 1"));
        }
        if api_version.get_minor() < 2 {
            errors.push(String::from("Device API minor version is less than 2"));
        }

        // If we get api version errors we cannot proceed to process it
        if !errors.is_empty() {
            return Ok(Self {
                name,
                api_version,
                uuid: properties.pipeline_cache_uuid,
                physical_device,
                config: None,
                warnings: warnings.into_boxed_slice(),
                errors: errors.into_boxed_slice(),
            })
        }

        let supported_extensions: HashSet<_> = unsafe {
            instance.enumerate_device_extension_properties(physical_device)
        }.map_err(|err| {
            log::error!("Failed to enumerate device ({}) extension properties: {:?}", name, err);
            err
        })?.into_iter().map(|ext| CString::from(unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) } )).collect();

        let mut vk_11_features = vk::PhysicalDeviceVulkan11Features::builder();
        let mut vk_11_properties = vk::PhysicalDeviceVulkan11Properties::builder();

        let mut khr_buffer_device_address_features = supported_extensions.get(ash::extensions::khr::BufferDeviceAddress::name()).map(|_| {
            vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::builder()
        });
        let mut khr_synchronization_2_features = supported_extensions.get(ash::extensions::khr::Synchronization2::name()).map(|_| {
            vk::PhysicalDeviceSynchronization2FeaturesKHR::builder()
        });
        let mut khr_timeline_semaphore_features_properties = supported_extensions.get(ash::extensions::khr::TimelineSemaphore::name()).map(|_| {
            (vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR::builder(), vk::PhysicalDeviceTimelineSemaphorePropertiesKHR::builder())
        });
        let mut khr_maintenance_4_features_properties = supported_extensions.get(ash::extensions::khr::Maintenance4::name()).map(|_| {
            (vk::PhysicalDeviceMaintenance4FeaturesKHR::builder(), vk::PhysicalDeviceMaintenance4PropertiesKHR::builder())
        });
        let mut khr_portability_subset_features_properties = supported_extensions.get(CStr::from_bytes_with_nul(b"VK_KHR_portability_subset\0").unwrap()).map(|_| {
            (vk::PhysicalDevicePortabilitySubsetFeaturesKHR::builder(), vk::PhysicalDevicePortabilitySubsetPropertiesKHR::builder())
        });

        let mut features2 = vk::PhysicalDeviceFeatures2::builder()
            .push_next(&mut vk_11_features);
        let mut properties2 = vk::PhysicalDeviceProperties2::builder()
            .push_next(&mut vk_11_properties);

        if let Some(f) = &mut khr_buffer_device_address_features {
            features2 = features2.push_next(f);
        }
        if let Some(f) = &mut khr_synchronization_2_features {
            features2 = features2.push_next(f);
        }
        if let Some((f, p)) = &mut khr_timeline_semaphore_features_properties {
            features2 = features2.push_next(f);
            properties2 = properties2.push_next(p);
        }
        if let Some((f, p)) = &mut khr_maintenance_4_features_properties {
            features2 = features2.push_next(f);
            properties2 = properties2.push_next(p);
        }
        if let Some((f, p)) = &mut khr_portability_subset_features_properties {
            features2 = features2.push_next(f);
            properties2 = properties2.push_next(p);
        }

        unsafe {
            instance.get_physical_device_features2(physical_device, &mut features2);
            instance.get_physical_device_properties2(physical_device, &mut properties2);
        }

        let vk_10_features = features2.features;
        let vk_10_properties = properties2.properties;
        drop(features2);
        drop(properties2);

        let vk_10 = Self::process_vk_10(&mut warnings, &mut errors, &vk_10_features, &vk_10_properties);
        let vk_11 = Self::process_vk_11(&mut warnings, &mut errors, &vk_11_features, &vk_11_properties);
        let khr_buffer_device_address = Self::process_khr_buffer_device_address(&mut warnings, &mut errors, khr_buffer_device_address_features.as_ref());
        let khr_synchronization_2 = Self::process_khr_synchronization_2(&mut warnings, &mut errors, khr_synchronization_2_features.as_ref());
        let khr_timeline_semaphore = Self::process_khr_timeline_semaphore(&mut warnings, &mut errors, khr_timeline_semaphore_features_properties.as_ref());
        let khr_maintenance_4 = Self::process_khr_maintenance_4(&mut warnings, &mut errors, khr_maintenance_4_features_properties.as_ref());
        let khr_portability_subset = Self::process_khr_portability_subset(&mut warnings, &mut errors, khr_portability_subset_features_properties.as_ref());

        let queue_properties = unsafe {
            instance.get_physical_device_queue_family_properties(physical_device)
        };

        let mut main_queue = None;
        let mut compute_queue = None;
        let mut transfer_queue = None;

        for (index, properties) in queue_properties.iter().enumerate() {
            if properties.queue_flags.contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER) {
                main_queue = Some(index as u32);
                break;
            }
        }
        if let Some(main_queue) = main_queue {
            for (index, properties) in queue_properties.iter().enumerate() {
                let index = index as u32;
                if index == main_queue {
                    continue;
                }

                if properties.queue_flags.contains(vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER) {
                    compute_queue = Some((index, false));
                    break;
                }
            }

            for (index, properties) in queue_properties.iter().enumerate() {
                let index = index as u32;
                if index == main_queue || compute_queue.map(|(q, _)| q) == Some(index) {
                    continue;
                }

                if properties.queue_flags.contains(vk::QueueFlags::TRANSFER) {
                    let g = properties.min_image_transfer_granularity;
                    if g.width == 1 && g.height == 1 && g.depth == 1 {
                        transfer_queue = Some((index, false, None));
                    } else {
                        transfer_queue = Some((index, false, Some(g)));
                    }
                    break;
                }
            }
        } else {
            errors.push(String::from("Failed to find queue with `GRAPHICS`, `COMPUTE` and `TRANSFER` capabilities"));
        }
        if compute_queue.is_none() {
            warnings.push(String::from("No suitable dedicated compute queue"));
        }
        if transfer_queue.is_none() {
            warnings.push(String::from("No suitable dedicated transfer queue"));
        }

        let mut enabled_extensions = HashSet::new();
        if khr_buffer_device_address.is_some() {
            enabled_extensions.insert(CString::from(ash::extensions::khr::BufferDeviceAddress::name()));
        }
        if khr_synchronization_2.is_some() {
            enabled_extensions.insert(CString::from(ash::extensions::khr::Synchronization2::name()));
        }
        if khr_timeline_semaphore.is_some() {
            enabled_extensions.insert(CString::from(ash::extensions::khr::TimelineSemaphore::name()));
        }
        if khr_maintenance_4.is_some() {
            enabled_extensions.insert(CString::from(ash::extensions::khr::Maintenance4::name()));
        }
        if khr_portability_subset.is_some() {
            enabled_extensions.insert(CString::from(CStr::from_bytes_with_nul(b"VK_KHR_portability_subset\0").unwrap()));
        }
        if supported_extensions.contains(ash::extensions::khr::Swapchain::name()) && khr_surface.is_some() {
            enabled_extensions.insert(CString::from(ash::extensions::khr::Swapchain::name()));
        }

        let config = if errors.is_empty() {
            let features = MainDeviceFeatures {
                vk_10,
                vk_11,
                khr_buffer_device_address: khr_buffer_device_address.unwrap(),
                khr_synchronization_2: khr_synchronization_2.unwrap(),
                khr_timeline_semaphore: khr_timeline_semaphore.unwrap(),
                khr_maintenance_4,
                khr_portability_subset,
            };

            Some(MainDeviceConfig {
                features,
                extensions: enabled_extensions,
                main_queue: main_queue.unwrap(),
                compute_queue,
                transfer_queue,
            })
        } else {
            None
        };

        Ok(Self {
            name,
            api_version,
            uuid: properties.pipeline_cache_uuid,
            physical_device,
            config,
            warnings: warnings.into_boxed_slice(),
            errors: errors.into_boxed_slice(),
        })
    }

    pub fn create_device(&self, instance: Arc<InstanceContext>) -> Result<MainDeviceContext, DeviceCreateError> {
        if let Some(config) = &self.config {
            let priorities = [1f32];
            let mut queue_create_infos = Vec::with_capacity(3);
            queue_create_infos.push({
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(config.main_queue)
                    .queue_priorities(&priorities)
                    .build()
            });
            if let Some((index, _)) = &config.compute_queue {
                queue_create_infos.push({
                    vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(*index)
                        .queue_priorities(&priorities)
                        .build()
                })
            }
            if let Some((index, _, _)) = &config.transfer_queue {
                queue_create_infos.push({
                    vk::DeviceQueueCreateInfo::builder()
                        .queue_family_index(*index)
                        .queue_priorities(&priorities)
                        .build()
                })
            }

            let extensions: Box<[_]> = config.extensions.iter().map(|ext| ext.as_ptr()).collect();

            let mut create_info = vk::DeviceCreateInfo::builder()
                .queue_create_infos(&queue_create_infos)
                .enabled_extension_names(&extensions)
                .enabled_features(&config.features.vk_10);

            let mut vk_11_features = config.features.vk_11.clone();
            vk_11_features.p_next = std::ptr::null_mut();
            create_info = create_info.push_next(&mut vk_11_features);

            let mut khr_buffer_device_address_features = config.features.khr_buffer_device_address.clone();
            khr_buffer_device_address_features.p_next = std::ptr::null_mut();
            create_info = create_info.push_next(&mut khr_buffer_device_address_features);

            let mut khr_synchronization_2_features = config.features.khr_synchronization_2.clone();
            khr_synchronization_2_features.p_next = std::ptr::null_mut();
            create_info = create_info.push_next(&mut khr_synchronization_2_features);

            let mut khr_timeline_semaphore_features = config.features.khr_timeline_semaphore.clone();
            khr_timeline_semaphore_features.p_next = std::ptr::null_mut();
            create_info = create_info.push_next(&mut khr_timeline_semaphore_features);

            let mut khr_maintenance_4_features = config.features.khr_maintenance_4.clone();
            if let Some(f) = &mut khr_maintenance_4_features {
                f.p_next = std::ptr::null_mut();
                create_info = create_info.push_next(f);
            }

            let mut khr_portability_subset_features = config.features.khr_portability_subset.clone();
            if let Some(f) = &mut khr_portability_subset_features {
                f.p_next = std::ptr::null_mut();
                create_info = create_info.push_next(f);
            }

            let device = unsafe {
                instance.get_instance().create_device(self.physical_device, &create_info, None)
            }.map_err(|err| {
                log::info!("Failed to create physical device: {:?}", err);
                err
            })?;

            let main_queue = DeviceQueue::new(unsafe { device.get_device_queue(config.main_queue, 0) }, config.main_queue);
            let compute_queue = config.compute_queue.map(|(family, _)| {
                DeviceQueue::new(unsafe { device.get_device_queue(family, 0) }, family)
            });
            let transfer_queue = config.transfer_queue.map(|(family, _, _)| {
                DeviceQueue::new(unsafe { device.get_device_queue(family, 0) }, family)
            });

            let khr_buffer_device_address = ash::extensions::khr::BufferDeviceAddress::new(instance.get_instance(), &device);
            let khr_synchronization_2 = ash::extensions::khr::Synchronization2::new(instance.get_instance(), &device);
            let khr_timeline_semaphore = ash::extensions::khr::TimelineSemaphore::new(instance.get_instance(), &device);
            let khr_maintenance_4 = config.features.khr_maintenance_4.map(|_| {
                ash::extensions::khr::Maintenance4::new(instance.get_instance(), &device)
            });
            let khr_swapchain = config.extensions.get(ash::extensions::khr::Swapchain::name()).map(|_| {
                ash::extensions::khr::Swapchain::new(instance.get_instance(), &device)
            });

            Ok(MainDeviceContext {
                instance,
                physical_device: self.physical_device,
                device,
                khr_buffer_device_address,
                khr_synchronization_2,
                khr_timeline_semaphore,
                khr_maintenance_4,
                khr_swapchain,
                enabled_extensions: config.extensions.clone(),
                main_queue,
                compute_queue,
                transfer_queue,
            })
        } else {
            Err(DeviceCreateError::NotSupported)
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_uuid(&self) -> &[u8; vk::UUID_SIZE] {
        &self.uuid
    }

    pub fn is_suitable(&self) -> bool {
        self.config.is_some()
    }

    pub fn get_warnings(&self) -> Option<&[String]> {
        if !self.warnings.is_empty() {
            Some(&self.warnings)
        } else {
            None
        }
    }

    pub fn get_errors(&self) -> Option<&[String]> {
        if !self.errors.is_empty() {
            Some(&self.errors)
        } else {
            None
        }
    }

    fn process_vk_10(warnings: &mut Vec<String>, errors: &mut Vec<String>, features: &vk::PhysicalDeviceFeatures, _properties: &vk::PhysicalDeviceProperties) -> vk::PhysicalDeviceFeatures {
        let mut enabled = vk::PhysicalDeviceFeatures::builder();

        if features.independent_blend == vk::TRUE {
            enabled.independent_blend = vk::TRUE;
        } else {
            errors.push(String::from("Feature `independent_blend` is not supported"));
        }

        if features.dual_src_blend == vk::TRUE {
            enabled.dual_src_blend = vk::TRUE;
        } else {
            errors.push(String::from("Feature `dual_src_blend` is not supported"));
        }

        if features.sampler_anisotropy == vk::TRUE {
            enabled.sampler_anisotropy = vk::TRUE;
        } else {
            warnings.push(String::from("Feature `sampler_anisotropy` is not supported"));
        }

        if features.fragment_stores_and_atomics == vk::TRUE {
            enabled.fragment_stores_and_atomics = vk::TRUE;
        } else {
            errors.push(String::from("Feature `fragment_stores_and_atomics` is not supported"));
        }

        if features.shader_int64 == vk::TRUE {
            enabled.shader_int64 = vk::TRUE;
        } else {
            errors.push(String::from("Feature `shader_int64` is not supported"));
        }

        enabled.build()
    }

    fn process_vk_11(_warnings: &mut Vec<String>, errors: &mut Vec<String>, features: &vk::PhysicalDeviceVulkan11FeaturesBuilder, _properties: &vk::PhysicalDeviceVulkan11PropertiesBuilder) -> vk::PhysicalDeviceVulkan11Features {
        let mut enabled = vk::PhysicalDeviceVulkan11Features::builder();

        if features.variable_pointers_storage_buffer == vk::TRUE {
            enabled.variable_pointers_storage_buffer = vk::TRUE;
        } else {
            errors.push(String::from("Feature `variable_pointers_storage_buffer` is not supported"));
        }

        if features.variable_pointers == vk::TRUE {
            enabled.variable_pointers = vk::TRUE;
        } else {
            errors.push(String::from("Feature `variable_pointers` is not supported"));
        }

        enabled.build()
    }

    fn process_khr_buffer_device_address(_warnings: &mut Vec<String>, errors: &mut Vec<String>, ext: Option<&vk::PhysicalDeviceBufferDeviceAddressFeaturesBuilder>) -> Option<vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR> {
        if let Some(f) = ext {
            let mut ok = true;
            let mut enabled = vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR::builder();

            if f.buffer_device_address == vk::TRUE {
                enabled.buffer_device_address = vk::TRUE;
            } else {
                errors.push(String::from("Feature `buffer_device_address` is not supported"));
                ok = false;
            }

            if ok {
                Some(enabled.build())
            } else {
                None
            }
        } else {
            errors.push(String::from("Extension `VK_KHR_buffer_device_address` is not supported"));
            None
        }
    }

    fn process_khr_synchronization_2(_warnings: &mut Vec<String>, errors: &mut Vec<String>, ext: Option<&vk::PhysicalDeviceSynchronization2FeaturesBuilder>) -> Option<vk::PhysicalDeviceSynchronization2FeaturesKHR> {
        if let Some(f) = ext {
            let mut ok = true;
            let mut enabled = vk::PhysicalDeviceSynchronization2FeaturesKHR::builder();

            if f.synchronization2 == vk::TRUE {
                enabled.synchronization2 = vk::TRUE;
            } else {
                errors.push(String::from("Feature `synchronization2` is not supported"));
                ok = false;
            }

            if ok {
                Some(enabled.build())
            } else {
                None
            }
        } else {
            errors.push(String::from("Extension `VK_KHR_synchronization2` is not supported"));
            None
        }
    }

    fn process_khr_timeline_semaphore(_warnings: &mut Vec<String>, errors: &mut Vec<String>, ext: Option<&(vk::PhysicalDeviceTimelineSemaphoreFeaturesBuilder, vk::PhysicalDeviceTimelineSemaphorePropertiesBuilder)>) -> Option<vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR> {
        if let Some((f, p)) = ext {
            let mut ok = true;
            let mut enabled = vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR::builder();

            if f.timeline_semaphore == vk::TRUE {
                enabled.timeline_semaphore = vk::TRUE;
            } else {
                errors.push(String::from("Feature `timeline_semaphore` is not supported"));
                ok = false;
            }

            if p.max_timeline_semaphore_value_difference < (1u64 << 16) {
                errors.push(String::from("Limit `max_timeline_semaphore_value_difference` is lower than 2^16"));
                ok = false;
            }

            if ok {
                Some(enabled.build())
            } else {
                None
            }
        } else {
            errors.push(String::from("Extension `VK_KHR_timeline_semaphore` is not supported"));
            None
        }
    }

    fn process_khr_maintenance_4(warnings: &mut Vec<String>, _errors: &mut Vec<String>, ext: Option<&(vk::PhysicalDeviceMaintenance4FeaturesBuilder, vk::PhysicalDeviceMaintenance4PropertiesBuilder)>) -> Option<vk::PhysicalDeviceMaintenance4FeaturesKHR> {
        if let Some((f, _p)) = ext {
            let mut ok = true;
            let mut enabled = vk::PhysicalDeviceMaintenance4FeaturesKHR::builder();

            if f.maintenance4 == vk::TRUE {
                enabled.maintenance4 = vk::TRUE;
            } else {
                warnings.push(String::from("Feature `maintenance4` is not supported"));
                ok = false;
            }

            if ok {
                Some(enabled.build())
            } else {
                None
            }
        } else {
            warnings.push(String::from("Extension `VK_KHR_maintenance4` is not supported"));
            None
        }
    }

    fn process_khr_portability_subset(_warnings: &mut Vec<String>, errors: &mut Vec<String>, ext: Option<&(vk::PhysicalDevicePortabilitySubsetFeaturesKHRBuilder, vk::PhysicalDevicePortabilitySubsetPropertiesKHRBuilder)>) -> Option<vk::PhysicalDevicePortabilitySubsetFeaturesKHR> {
        if let Some((f, _p)) = ext {
            let mut ok = true;
            let mut enabled = vk::PhysicalDevicePortabilitySubsetFeaturesKHR::builder();

            if f.constant_alpha_color_blend_factors == vk::TRUE {
                enabled.constant_alpha_color_blend_factors = vk::TRUE;
            } else {
                errors.push(String::from("Portability subset feature `constant_alpha_color_blend_factors` is not supported"));
                ok = false;
            }

            if f.events == vk::TRUE {
                enabled.events = vk::TRUE;
            } else {
                errors.push(String::from("Portability subset feature `events` is not supported"));
                ok = false;
            }

            if ok {
                Some(enabled.build())
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl std::fmt::Debug for MainDeviceReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainDeviceReport")
            .field("device_name", &self.name)
            .field("api_version", &self.api_version)
            .field("suitable", &self.is_suitable())
            .field("warnings", &self.warnings.as_ref())
            .field("errors", &self.errors.as_ref())
            .finish()
    }
}

struct MainDeviceConfig {
    features: MainDeviceFeatures,
    extensions: HashSet<CString>,
    main_queue: u32,
    compute_queue: Option<(u32, bool)>,
    transfer_queue: Option<(u32, bool, Option<vk::Extent3D>)>,
}

struct MainDeviceFeatures {
    vk_10: vk::PhysicalDeviceFeatures,
    vk_11: vk::PhysicalDeviceVulkan11Features,
    khr_buffer_device_address: vk::PhysicalDeviceBufferDeviceAddressFeaturesKHR,
    khr_synchronization_2: vk::PhysicalDeviceSynchronization2FeaturesKHR,
    khr_timeline_semaphore: vk::PhysicalDeviceTimelineSemaphoreFeaturesKHR,
    khr_maintenance_4: Option<vk::PhysicalDeviceMaintenance4FeaturesKHR>,
    khr_portability_subset: Option<vk::PhysicalDevicePortabilitySubsetFeaturesKHR>,
}