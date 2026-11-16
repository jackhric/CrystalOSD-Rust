use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString};
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU32, Ordering};

use ash::vk;

use crate::bootstrap::{self, BuiltInstance, InstanceRequest, RequiredFeatures, Selector};
use crate::error::{Error, Result, check, runtime};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceOptions {
    pub validation: bool,
    pub sync_validation: bool,
}

// What Device takes from the window (SDL3 in the app): SDL_Vulkan_GetInstanceExtensions, SDL_Vulkan_CreateSurface,
// SDL_GetWindowSizeInPixels and SDL_WINDOW_MINIMIZED. The surface is destroyed by Device.
pub trait Window {
    fn instance_extensions(&self) -> Vec<CString>;
    fn create_surface(&self, entry: &ash::Entry, instance: &ash::Instance) -> std::result::Result<vk::SurfaceKHR, String>;
    fn size_in_pixels(&self) -> (i32, i32);
    fn minimized(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameContext {
    pub cmd: vk::CommandBuffer,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub index: u32,
}

const FRAMES_IN_FLIGHT: usize = 2;

#[derive(Default, Clone, Copy)]
struct Frame {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    acquired: vk::Semaphore,
    fence: vk::Fence,
}

#[derive(Default)]
struct Swapchain {
    swapchain: vk::SwapchainKHR,
    format: vk::Format,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    render_done: Vec<vk::Semaphore>,
}

pub struct Device {
    window: Option<Box<dyn Window>>,
    instance: BuiltInstance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue_families: Vec<vk::QueueFamilyProperties>,
    surface: vk::SurfaceKHR,
    surface_loader: ash::khr::surface::Instance,
    swapchain_loader: Option<ash::khr::swapchain::Device>,
    swapchain: RefCell<Swapchain>,
    allocator: ManuallyDrop<vk_mem::Allocator>,
    queue: vk::Queue,
    queue_family: u32,
    frames: [Frame; FRAMES_IN_FLIGHT],
    frame_slot: Cell<usize>,
    has_swapchain: Cell<bool>,
    sample_counts: vk::SampleCountFlags,
    errors: Box<AtomicU32>,
    debug_utils: Option<ash::ext::debug_utils::Device>,
    fault_info: Option<ash::ext::device_fault::Device>,
    push_descriptor: vk::PFN_vkCmdPushDescriptorSetKHR,
}

impl Device {
    pub fn new(window: Option<Box<dyn Window>>, options: &DeviceOptions) -> Result<Device> {
        let errors = Box::new(AtomicU32::new(0));
        let request = InstanceRequest {
            app_name: c"CrystalOSD",
            headless: window.is_none(),
            extensions: window.as_ref().map(|w| w.instance_extensions()).unwrap_or_default(),
            validation: options.validation,
            sync_validation: options.sync_validation,
            errors: &*errors,
        };
        let instance = bootstrap::build_instance(&request)?;
        let surface_loader = ash::khr::surface::Instance::new(&instance.entry, &instance.instance);
        let destroy_instance = |instance: &BuiltInstance, surface: vk::SurfaceKHR| {
            // SAFETY: nothing else was created from the instance.
            unsafe {
                if surface != vk::SurfaceKHR::null() {
                    surface_loader.destroy_surface(surface, None);
                }
                instance.destroy();
            }
        };

        let mut surface = vk::SurfaceKHR::null();
        if let Some(w) = &window {
            match w.create_surface(&instance.entry, &instance.instance) {
                Ok(s) => surface = s,
                Err(e) => {
                    destroy_instance(&instance, surface);
                    return Err(runtime(format!("surface: {e}")));
                }
            }
        }

        let selector = Selector {
            surface,
            minimum_version: vk::make_api_version(0, 1, 4, 0),
            features: RequiredFeatures { vulkan13: Some((true, true)), vulkan14_push_descriptor: Some(true), ..Default::default() },
            required_extensions: Vec::new(),
        };
        let mut chosen = match selector.select(&instance) {
            Ok(c) => c,
            Err(e) => {
                destroy_instance(&instance, surface);
                return Err(e);
            }
        };
        let limits = &chosen.properties.limits;
        let sample_counts = limits.framebuffer_color_sample_counts & limits.framebuffer_depth_sample_counts;
        let fault = chosen.enable_fault_if_present(&instance.instance);

        let built = match bootstrap::build_device(&instance, &chosen) {
            Ok(d) => d,
            Err(e) => {
                destroy_instance(&instance, surface);
                return Err(e);
            }
        };
        let fault_info = fault.then(|| ash::ext::device_fault::Device::new(&instance.instance, &built.device));
        let (queue, queue_family) = bootstrap::graphics_queue(&built)?;
        let allocator = bootstrap::allocator(&instance, &built.device, chosen.physical_device)?;
        let debug_utils =
            (instance.messenger != vk::DebugUtilsMessengerEXT::null()).then(|| ash::ext::debug_utils::Device::new(&instance.instance, &built.device));
        // SAFETY: vkCmdPushDescriptorSet is core in Vulkan 1.4 with the signature of vkCmdPushDescriptorSetKHR.
        let push_descriptor: vk::PFN_vkCmdPushDescriptorSetKHR = unsafe {
            let f = (instance.instance.fp_v1_0().get_device_proc_addr)(built.device.handle(), c"vkCmdPushDescriptorSet".as_ptr())
                .ok_or_else(|| runtime("vkCmdPushDescriptorSet"))?;
            std::mem::transmute::<unsafe extern "system" fn(), vk::PFN_vkCmdPushDescriptorSetKHR>(f)
        };

        let mut device = Device {
            swapchain_loader: window.as_ref().map(|_| ash::khr::swapchain::Device::new(&instance.instance, &built.device)),
            window,
            device: built.device,
            physical_device: chosen.physical_device,
            queue_families: built.queue_families,
            surface,
            surface_loader,
            swapchain: RefCell::new(Swapchain::default()),
            allocator: ManuallyDrop::new(allocator),
            queue,
            queue_family,
            frames: [Frame::default(); FRAMES_IN_FLIGHT],
            frame_slot: Cell::new(0),
            has_swapchain: Cell::new(false),
            sample_counts,
            errors,
            debug_utils,
            fault_info,
            push_descriptor,
            instance,
        };
        if device.window.is_none() {
            return Ok(device);
        }

        for i in 0..FRAMES_IN_FLIGHT {
            let d = &device.device;
            let pool_info = vk::CommandPoolCreateInfo::default().flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER).queue_family_index(queue_family);
            // SAFETY: creating objects on a valid device; Drop destroys them.
            unsafe {
                let pool = check(d.create_command_pool(&pool_info, None), "command pool")?;
                device.frames[i].pool = pool;
                let alloc = vk::CommandBufferAllocateInfo::default().command_pool(pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1);
                device.frames[i].cmd = check(d.allocate_command_buffers(&alloc), "command buffer")?[0];
                device.frames[i].acquired = check(d.create_semaphore(&vk::SemaphoreCreateInfo::default(), None), "semaphore")?;
                device.frames[i].fence = check(d.create_fence(&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED), None), "fence")?;
            }
        }
        let created = device.create_swapchain()?;
        device.has_swapchain.set(created);
        Ok(device)
    }

    pub fn entry(&self) -> &ash::Entry {
        &self.instance.entry
    }
    pub fn instance(&self) -> &ash::Instance {
        &self.instance.instance
    }
    pub fn device(&self) -> &ash::Device {
        &self.device
    }
    pub fn physical_device(&self) -> vk::PhysicalDevice {
        self.physical_device
    }
    pub fn allocator(&self) -> &vk_mem::Allocator {
        &self.allocator
    }
    pub fn queue(&self) -> vk::Queue {
        self.queue
    }
    pub fn queue_family(&self) -> u32 {
        self.queue_family
    }
    pub fn queue_families(&self) -> &[vk::QueueFamilyProperties] {
        &self.queue_families
    }
    pub fn swapchain_format(&self) -> vk::Format {
        self.swapchain.borrow().format
    }
    pub fn swapchain_extent(&self) -> vk::Extent2D {
        self.swapchain.borrow().extent
    }
    pub fn swapchain_image_count(&self) -> u32 {
        self.swapchain.borrow().images.len() as u32
    }
    pub fn sample_counts(&self) -> vk::SampleCountFlags {
        self.sample_counts
    }
    pub fn validation_errors(&self) -> u32 {
        self.errors.load(Ordering::SeqCst)
    }
    pub fn labels(&self) -> bool {
        self.debug_utils.is_some()
    }

    pub fn begin_label(&self, cmd: vk::CommandBuffer, name: &str) {
        let Some(d) = &self.debug_utils else { return };
        let name = CString::new(name.replace('\0', " ")).unwrap_or_default();
        let label = vk::DebugUtilsLabelEXT::default().label_name(&name);
        // SAFETY: `cmd` is recording.
        unsafe { d.cmd_begin_debug_utils_label(cmd, &label) };
    }

    pub fn end_label(&self, cmd: vk::CommandBuffer) {
        if let Some(d) = &self.debug_utils {
            // SAFETY: `cmd` is recording, inside a label.
            unsafe { d.cmd_end_debug_utils_label(cmd) };
        }
    }

    pub(crate) unsafe fn cmd_push_descriptor_set(&self, cmd: vk::CommandBuffer, layout: vk::PipelineLayout, set: u32, writes: &[vk::WriteDescriptorSet<'_>]) {
        // SAFETY: the caller records into `cmd` with a layout of push descriptors.
        unsafe { (self.push_descriptor)(cmd, vk::PipelineBindPoint::GRAPHICS, layout, set, writes.len() as u32, writes.as_ptr()) };
    }

    pub fn fault_report(&self) -> String {
        let Some(f) = &self.fault_info else { return String::new() };
        let mut counts = vk::DeviceFaultCountsEXT::default();
        // SAFETY: a query on a lost or live device, with locals for its outputs.
        unsafe {
            if (f.fp().get_device_fault_info_ext)(self.device.handle(), &mut counts, std::ptr::null_mut()) != vk::Result::SUCCESS {
                return String::new();
            }
            let mut addresses = vec![vk::DeviceFaultAddressInfoEXT::default(); counts.address_info_count as usize];
            let mut vendors = vec![vk::DeviceFaultVendorInfoEXT::default(); counts.vendor_info_count as usize];
            let mut info = vk::DeviceFaultInfoEXT { p_address_infos: addresses.as_mut_ptr(), p_vendor_infos: vendors.as_mut_ptr(), ..Default::default() };
            if (f.fp().get_device_fault_info_ext)(self.device.handle(), &mut counts, &mut info).as_raw() < 0 {
                return String::new();
            }
            let text = |c: &[std::ffi::c_char]| CStr::from_ptr(c.as_ptr()).to_string_lossy().into_owned();
            let mut report = format!("\ndevice fault: {}", text(&info.description));
            for a in &addresses {
                report += &format!("\n  address type {} at 0x{:x}, precision {}", a.address_type.as_raw(), a.reported_address, a.address_precision);
            }
            for v in &vendors {
                report += &format!("\n  vendor {} code 0x{:x} data 0x{:x}", text(&v.description), v.vendor_fault_code, v.vendor_fault_data);
            }
            report
        }
    }

    pub fn verify<T>(&self, result: ash::prelude::VkResult<T>, what: &str) -> Result<T> {
        match result {
            Err(vk::Result::ERROR_DEVICE_LOST) => Err(Error::DeviceLost(format!("{what}: device lost{}", self.fault_report()))),
            r => check(r, what),
        }
    }

    fn destroy_swapchain_views(&self, s: &mut Swapchain) {
        // SAFETY: the device is idle (callers wait) and nothing uses these objects.
        unsafe {
            for &view in &s.views {
                self.device.destroy_image_view(view, None);
            }
            for &semaphore in &s.render_done {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        s.views.clear();
        s.render_done.clear();
        s.images.clear();
    }

    // vkb::SwapchainBuilder with the desired format B8G8R8A8_UNORM / SRGB_NONLINEAR (else the first), FIFO, the window's
    // pixel size (the surface's current extent when it has one), minImageCount + 1 images, colour attachment and
    // transfer dst usage, the current transform, opaque composite alpha, clipped, the old swapchain.
    fn create_swapchain(&self) -> Result<bool> {
        let window = self.window.as_ref().expect("a window");
        let (width, height) = window.size_in_pixels();
        if width <= 0 || height <= 0 {
            return Ok(false);
        }
        let loader = self.swapchain_loader.as_ref().expect("a swapchain loader");
        let mut s = self.swapchain.borrow_mut();
        // SAFETY: a valid device.
        unsafe { self.device.device_wait_idle() }.ok();
        self.destroy_swapchain_views(&mut s);

        let present_family = (0..self.queue_families.len() as u32)
            .find(|&i| {
                // SAFETY: a plain query.
                unsafe { self.surface_loader.get_physical_device_surface_support(self.physical_device, i, self.surface) }.unwrap_or(false)
            })
            .unwrap_or(u32::MAX);
        // SAFETY: plain queries with a valid surface.
        let (capabilities, formats, modes) = unsafe {
            let c = self.surface_loader.get_physical_device_surface_capabilities(self.physical_device, self.surface);
            let f = self.surface_loader.get_physical_device_surface_formats(self.physical_device, self.surface);
            let m = self.surface_loader.get_physical_device_surface_present_modes(self.physical_device, self.surface);
            match (c, f, m) {
                (Ok(c), Ok(f), Ok(m)) => (c, f, m),
                _ => return Err(runtime("swapchain: failed_query_surface_support_details")),
            }
        };
        let mut image_count = capabilities.min_image_count + 1;
        if capabilities.max_image_count > 0 && image_count > capabilities.max_image_count {
            image_count = capabilities.max_image_count;
        }
        let desired = vk::SurfaceFormatKHR { format: vk::Format::B8G8R8A8_UNORM, color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR };
        let format =
            formats.iter().copied().find(|f| f.format == desired.format && f.color_space == desired.color_space).map(|_| desired).unwrap_or(formats[0]);
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: capabilities.min_image_extent.width.max(capabilities.max_image_extent.width.min(width as u32)),
                height: capabilities.min_image_extent.height.max(capabilities.max_image_extent.height.min(height as u32)),
            }
        };
        let layers = if capabilities.max_image_array_layers < 1 { capabilities.max_image_array_layers } else { 1 };
        let _ = modes;
        let present_mode = vk::PresentModeKHR::FIFO;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST;
        if (usage & capabilities.supported_usage_flags) != usage {
            return Err(runtime("swapchain: required_usage_not_supported"));
        }
        let families = [self.queue_family, present_family];
        let mut info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(layers)
            .image_usage(usage)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(s.swapchain);
        info = if self.queue_family != present_family {
            info.image_sharing_mode(vk::SharingMode::CONCURRENT).queue_family_indices(&families)
        } else {
            info.image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        };
        // SAFETY: a valid surface and device; the old swapchain is retired here and destroyed below.
        let swapchain = unsafe { loader.create_swapchain(&info, None) }.map_err(|_| runtime("swapchain: failed_create_swapchain"))?;
        if s.swapchain != vk::SwapchainKHR::null() {
            // SAFETY: the device is idle and the old swapchain retired.
            unsafe { loader.destroy_swapchain(s.swapchain, None) };
        }
        s.swapchain = swapchain;
        s.format = format.format;
        s.extent = extent;
        // SAFETY: a valid swapchain.
        s.images = check(unsafe { loader.get_swapchain_images(swapchain) }, "swapchain images")?;
        for &image in &s.images.clone() {
            let mut usage_info = vk::ImageViewUsageCreateInfo::default().usage(usage);
            let view = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .components(vk::ComponentMapping::default())
                .subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1))
                .push_next(&mut usage_info);
            // SAFETY: a swapchain image of this device.
            s.views.push(check(unsafe { self.device.create_image_view(&view, None) }, "swapchain image views")?);
        }
        for _ in 0..s.images.len() {
            // SAFETY: a valid device.
            s.render_done.push(check(unsafe { self.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }, "semaphore")?);
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &self,
        cmd: vk::CommandBuffer,
        image: vk::Image,
        from: vk::ImageLayout,
        to: vk::ImageLayout,
        src_stage: vk::PipelineStageFlags2,
        src_access: vk::AccessFlags2,
        dst_stage: vk::PipelineStageFlags2,
        dst_access: vk::AccessFlags2,
    ) {
        crate::native_renderer::barrier(&self.device, cmd, image, vk::ImageAspectFlags::COLOR, from, to, src_stage, src_access, dst_stage, dst_access);
    }

    pub fn begin_frame(&self) -> Result<Option<FrameContext>> {
        let Some(window) = &self.window else { return Ok(None) };
        let (width, height) = window.size_in_pixels();
        if width <= 0 || height <= 0 || window.minimized() {
            return Ok(None);
        }
        let extent = self.swapchain_extent();
        if !self.has_swapchain.get() || extent.width != width as u32 || extent.height != height as u32 {
            self.has_swapchain.set(self.create_swapchain()?);
        }
        if !self.has_swapchain.get() {
            return Ok(None);
        }

        let frame = self.frames[self.frame_slot.get()];
        // SAFETY: the fence belongs to this device.
        self.verify(unsafe { self.device.wait_for_fences(&[frame.fence], true, u64::MAX) }, "wait fence")?;

        let loader = self.swapchain_loader.as_ref().expect("a swapchain loader");
        let swapchain = self.swapchain.borrow().swapchain;
        // SAFETY: a valid swapchain and an unsignalled semaphore.
        let acquired = unsafe { loader.acquire_next_image(swapchain, u64::MAX, frame.acquired, vk::Fence::null()) };
        let index = match acquired {
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.has_swapchain.set(self.create_swapchain()?);
                return Ok(None);
            }
            Ok((index, _suboptimal)) => index,
            Err(r) => return self.verify(Err(r), "acquire"),
        };
        // SAFETY: the fence is signalled and unused.
        self.verify(unsafe { self.device.reset_fences(&[frame.fence]) }, "reset fence")?;

        // SAFETY: the command buffer's last submission completed (its fence was waited on).
        unsafe {
            self.verify(self.device.reset_command_buffer(frame.cmd, vk::CommandBufferResetFlags::empty()), "reset command buffer")?;
            let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.verify(self.device.begin_command_buffer(frame.cmd, &begin), "begin command buffer")?;
        }
        let (image, view) = {
            let s = self.swapchain.borrow();
            (s.images[index as usize], s.views[index as usize])
        };
        self.transition(
            frame.cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        Ok(Some(FrameContext { cmd: frame.cmd, image, view, index }))
    }

    pub fn end_frame(&self, context: &FrameContext) -> Result<()> {
        let frame = self.frames[self.frame_slot.get()];
        self.transition(
            frame.cmd,
            context.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::NONE,
            vk::AccessFlags2::empty(),
        );
        // SAFETY: the command buffer is recording.
        self.verify(unsafe { self.device.end_command_buffer(frame.cmd) }, "end command buffer")?;

        let (swapchain, done) = {
            let s = self.swapchain.borrow();
            (s.swapchain, s.render_done[context.index as usize])
        };
        let wait = [vk::SemaphoreSubmitInfo::default().semaphore(frame.acquired).stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
        let signal = [vk::SemaphoreSubmitInfo::default().semaphore(done).stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
        let cmd = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
        let submit = vk::SubmitInfo2::default().wait_semaphore_infos(&wait).command_buffer_infos(&cmd).signal_semaphore_infos(&signal);
        // SAFETY: the command buffer is executable; the semaphores and fence belong to this device.
        self.verify(unsafe { self.device.queue_submit2(self.queue, &[submit], frame.fence) }, "submit")?;

        let loader = self.swapchain_loader.as_ref().expect("a swapchain loader");
        let swapchains = [swapchain];
        let indices = [context.index];
        let waits = [done];
        let present = vk::PresentInfoKHR::default().wait_semaphores(&waits).swapchains(&swapchains).image_indices(&indices);
        // SAFETY: the image was acquired and its rendering is signalled by `done`.
        let result = unsafe { loader.queue_present(self.queue, &present) };
        match result {
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Ok(true) => self.has_swapchain.set(self.create_swapchain()?),
            r => {
                self.verify(r, "present")?;
            }
        }
        self.frame_slot.set((self.frame_slot.get() + 1) % FRAMES_IN_FLIGHT);
        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: tearing down in the order Device::~Device does; nothing else uses these objects.
        unsafe {
            self.device.device_wait_idle().ok();
            if self.window.is_some() {
                let mut s = std::mem::take(&mut *self.swapchain.borrow_mut());
                self.destroy_swapchain_views(&mut s);
                if let Some(loader) = &self.swapchain_loader
                    && s.swapchain != vk::SwapchainKHR::null()
                {
                    loader.destroy_swapchain(s.swapchain, None);
                }
                for frame in &self.frames {
                    self.device.destroy_fence(frame.fence, None);
                    self.device.destroy_semaphore(frame.acquired, None);
                    self.device.destroy_command_pool(frame.pool, None);
                }
            }
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            if self.surface != vk::SurfaceKHR::null() {
                self.surface_loader.destroy_surface(self.surface, None);
            }
            self.instance.destroy();
        }
    }
}
