use std::marker::PhantomData;
use std::time::{Duration, Instant};

use ash::vk;

use crate::vulkan::device::{DeviceProvider, DeviceQueue, MainDeviceContext, SwapchainProvider};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[must_use]
pub enum NextImageResult {
    Ok,
    MustRecreate,
    Suboptimal,
    Timeout,
    VulkanError(vk::Result),
}

impl From<vk::Result> for NextImageResult {
    fn from(result: vk::Result) -> Self {
        Self::VulkanError(result)
    }
}

pub struct Swapchain<'a> {
    device: &'a ash::Device,
    swapchain_khr: &'a ash::extensions::khr::Swapchain,

    swapchain: vk::SwapchainKHR,
    images: Box<[SwapchainImage]>,

    acquire_fence: vk::Fence,

    acquire_semaphores: Box<[vk::Semaphore]>,
    next_acquire_semaphore: usize,

    _phantom_data: PhantomData<&'a ()>,
}

impl<'a> Swapchain<'a> {
    pub fn new(swapchain: vk::SwapchainKHR, device: &'a MainDeviceContext) -> Result<Self, vk::Result> {
        let swapchain_khr = device.get_swapchain_khr().unwrap();
        let device = device.get_device();

        let images_raw = unsafe {
            swapchain_khr.get_swapchain_images(swapchain)
        }?;

        let fence_create_info = vk::FenceCreateInfo::builder()
            .flags(vk::FenceCreateFlags::SIGNALED);

        let acquire_fence = unsafe {
            device.create_fence(&fence_create_info, None)
        }?;

        let semaphore_create_info = vk::SemaphoreCreateInfo::builder();
        let mut acquire_semaphores = Vec::with_capacity(images_raw.len());
        for _ in 0..images_raw.len() {
            let semaphore = unsafe {
                device.create_semaphore(&semaphore_create_info, None)
            }.map_err(|err| {
                unsafe {
                    device.destroy_fence(acquire_fence, None);
                    for semaphore in &acquire_semaphores {
                        device.destroy_semaphore(*semaphore, None)
                    };
                    err
                }
            })?;
            acquire_semaphores.push(semaphore);
        }

        let mut images: Vec<SwapchainImage> = Vec::with_capacity(images_raw.len());
        for image in images_raw.into_iter() {
            let image = SwapchainImage::new(image, device).map_err(|err| {
                unsafe {
                    device.destroy_fence(acquire_fence, None);
                    for semaphore in &acquire_semaphores {
                        device.destroy_semaphore(*semaphore, None)
                    };
                }
                for image in &images {
                    unsafe {
                        image.destroy(device);
                    }
                }
                err
            })?;
            images.push(image);
        }

        Ok(Self {
            device,
            swapchain_khr,
            swapchain,
            images: images.into_boxed_slice(),
            acquire_fence,
            acquire_semaphores: acquire_semaphores.into_boxed_slice(),
            next_acquire_semaphore: 0,
            _phantom_data: PhantomData,
        })
    }

    /// Attempts to acquire a image and calls the provided closure with it.
    pub fn with_next_image<'b, F>(&mut self, timeout: Duration, f: F) -> NextImageResult where
        F: FnOnce(&SwapchainImage, vk::Semaphore) -> Option<&'b DeviceQueue> {

        let start_instant = Instant::now();
        if let Err(result) = unsafe {
            self.device.wait_for_fences(std::slice::from_ref(&self.acquire_fence), true, timeout.as_nanos() as u64)
        } {
            return match result {
                vk::Result::TIMEOUT => NextImageResult::Timeout,
                _ => NextImageResult::VulkanError(result),
            }
        }

        if let Err(result) = unsafe {
            self.device.reset_fences(std::slice::from_ref(&self.acquire_fence))
        } {
            return NextImageResult::VulkanError(result);
        }

        let acquire_semaphore = self.acquire_semaphores[self.next_acquire_semaphore];

        let timeout = timeout - (Instant::now() - start_instant);
        let timeout = timeout.as_nanos() as u64;
        let (index, _) = match unsafe {
            self.swapchain_khr.acquire_next_image(self.swapchain, timeout, acquire_semaphore, self.acquire_fence)
        } {
            Ok(ok) => ok,
            Err(vk::Result::TIMEOUT) => return NextImageResult::Timeout,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return NextImageResult::MustRecreate,
            Err(result) => return NextImageResult::VulkanError(result),
        };
        self.next_acquire_semaphore = (self.next_acquire_semaphore + 1) % self.acquire_semaphores.len();

        let image = &self.images[index as usize];

        if let Some(queue) = f(image, acquire_semaphore) {
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(std::slice::from_ref(&image.present_semaphore))
                .swapchains(std::slice::from_ref(&self.swapchain))
                .image_indices(std::slice::from_ref(&index));

            let queue = queue.lock().unwrap();
            let result = unsafe {
                self.swapchain_khr.queue_present(*queue, &present_info)
            };
            drop(queue);

            match result {
                Ok(false) => NextImageResult::Ok,
                Ok(true) => NextImageResult::Suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => NextImageResult::MustRecreate,
                Err(result) => NextImageResult::VulkanError(result),
            }
        } else {
            NextImageResult::MustRecreate
        }
    }
}

impl<'a> Drop for Swapchain<'a> {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.wait_for_fences(std::slice::from_ref(&self.acquire_fence), true, u64::MAX).unwrap();

            for image in self.images.iter() {
                image.destroy(self.device);
            }
            for semaphore in self.acquire_semaphores.iter() {
                self.device.destroy_semaphore(*semaphore, None);
            }
            self.device.destroy_fence(self.acquire_fence, None);

            self.swapchain_khr.destroy_swapchain(self.swapchain, None);
        }
    }
}

pub struct SwapchainImage {
    /// The swapchain image.
    pub image: vk::Image,

    /// Semaphore signaled when rendering is done and the image can be presented.
    pub present_semaphore: vk::Semaphore,
}

impl SwapchainImage {
    fn new(image: vk::Image, device: &ash::Device) -> Result<Self, vk::Result> {
        let semaphore_create_info = vk::SemaphoreCreateInfo::builder();
        let present_semaphore = unsafe {
            device.create_semaphore(&semaphore_create_info, None)
        }?;

        Ok(Self {
            image,
            present_semaphore,
        })
    }

    fn destroy(&self, device: &ash::Device) {
        unsafe {
            device.destroy_semaphore(self.present_semaphore, None)
        };
    }
}