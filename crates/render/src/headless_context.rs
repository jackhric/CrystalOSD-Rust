use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU32, Ordering};

use ash::vk;

use crate::bootstrap::{self, BuiltInstance, InstanceRequest, RequiredFeatures, Selector};
use crate::error::Result;
use crate::gpu_device::GpuDevice;

pub struct HeadlessContext {
    instance: BuiltInstance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    allocator: ManuallyDrop<vk_mem::Allocator>,
    queue: vk::Queue,
    queue_family: u32,
    errors: Box<AtomicU32>,
}

impl HeadlessContext {
    pub fn new(validation: bool) -> Result<HeadlessContext> {
        let errors = Box::new(AtomicU32::new(0));
        let request =
            InstanceRequest { app_name: c"CrystalOSDParity", headless: true, extensions: Vec::new(), validation, sync_validation: false, errors: &*errors };
        let instance = bootstrap::build_instance(&request)?;
        let core = vk::PhysicalDeviceFeatures { fragment_stores_and_atomics: vk::TRUE, ..Default::default() };
        let selector = Selector {
            surface: vk::SurfaceKHR::null(),
            minimum_version: vk::make_api_version(0, 1, 4, 0),
            features: RequiredFeatures { core, vulkan13: Some((true, true)), interlock: Some(true), ..Default::default() },
            required_extensions: vec![ash::ext::fragment_shader_interlock::NAME.into()],
        };
        let built = selector.select(&instance).and_then(|physical| Ok((bootstrap::build_device(&instance, &physical)?, physical.physical_device)));
        let (built, physical_device) = match built {
            Ok(b) => b,
            Err(e) => {
                // SAFETY: nothing else was created from the instance.
                unsafe { instance.destroy() };
                return Err(e);
            }
        };
        let (queue, queue_family) = bootstrap::graphics_queue(&built)?;
        let allocator = bootstrap::allocator(&instance, &built.device, physical_device)?;
        Ok(HeadlessContext { instance, device: built.device, physical_device, allocator: ManuallyDrop::new(allocator), queue, queue_family, errors })
    }

    pub fn gpu(&self) -> GpuDevice<'_> {
        GpuDevice {
            device: &self.device,
            physical_device: self.physical_device,
            allocator: &self.allocator,
            queue: self.queue,
            queue_family: self.queue_family,
        }
    }

    pub fn instance(&self) -> &ash::Instance {
        &self.instance.instance
    }

    pub fn validation_errors(&self) -> u32 {
        self.errors.load(Ordering::SeqCst)
    }
}

impl Drop for HeadlessContext {
    fn drop(&mut self) {
        // SAFETY: the allocator, then the device, then the instance, as ~HeadlessContext.
        unsafe {
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            self.instance.destroy();
        }
    }
}
