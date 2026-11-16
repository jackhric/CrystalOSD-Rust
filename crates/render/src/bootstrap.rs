// What vk-bootstrap (build/_deps/vk-bootstrap-src) does for the calls Device.cpp and HeadlessContext.cpp make:
// InstanceBuilder::build, PhysicalDeviceSelector::select, DeviceBuilder::build, with its defaults where they matter.

use std::ffi::{CStr, CString, c_void};
use std::sync::atomic::{AtomicU32, Ordering};

use ash::vk;

use crate::error::{Error, Result, runtime};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

// VkPhysicalDeviceVulkan14Features (Vulkan 1.4; ash 0.38 has the 1.3 headers).
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PhysicalDeviceVulkan14Features {
    pub s_type: vk::StructureType,
    pub p_next: *mut c_void,
    pub global_priority_query: vk::Bool32,
    pub shader_subgroup_rotate: vk::Bool32,
    pub shader_subgroup_rotate_clustered: vk::Bool32,
    pub shader_float_controls2: vk::Bool32,
    pub shader_expect_assume: vk::Bool32,
    pub rectangular_lines: vk::Bool32,
    pub bresenham_lines: vk::Bool32,
    pub smooth_lines: vk::Bool32,
    pub stippled_rectangular_lines: vk::Bool32,
    pub stippled_bresenham_lines: vk::Bool32,
    pub stippled_smooth_lines: vk::Bool32,
    pub vertex_attribute_instance_rate_divisor: vk::Bool32,
    pub vertex_attribute_instance_rate_zero_divisor: vk::Bool32,
    pub index_type_uint8: vk::Bool32,
    pub dynamic_rendering_local_read: vk::Bool32,
    pub maintenance5: vk::Bool32,
    pub maintenance6: vk::Bool32,
    pub pipeline_protected_access: vk::Bool32,
    pub pipeline_robustness: vk::Bool32,
    pub host_image_copy: vk::Bool32,
    pub push_descriptor: vk::Bool32,
}

impl Default for PhysicalDeviceVulkan14Features {
    fn default() -> Self {
        // SAFETY: every field is an integer or a pointer, for which zero is valid.
        let mut s: Self = unsafe { std::mem::zeroed() };
        s.s_type = vk::StructureType::from_raw(55);
        s
    }
}

pub(crate) struct InstanceRequest<'a> {
    pub app_name: &'a CStr,
    pub headless: bool,
    pub extensions: Vec<CString>,
    pub validation: bool,
    pub sync_validation: bool,
    pub errors: *const AtomicU32,
}

pub(crate) struct BuiltInstance {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub debug_utils: Option<ash::ext::debug_utils::Instance>,
    pub messenger: vk::DebugUtilsMessengerEXT,
    pub headless: bool,
}

unsafe extern "system" fn on_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut c_void,
) -> vk::Bool32 {
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) && kind.contains(vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION) {
        // SAFETY: `user` is the AtomicU32 the instance was created with, alive until the messenger is destroyed;
        // `data` is valid for the call.
        unsafe {
            (*(user as *const AtomicU32)).fetch_add(1, Ordering::SeqCst);
            let message = if (*data).p_message.is_null() { "".into() } else { CStr::from_ptr((*data).p_message).to_string_lossy() };
            eprintln!("validation: {message}");
        }
    }
    vk::FALSE
}

fn names(properties: &[vk::ExtensionProperties]) -> Vec<CString> {
    properties.iter().map(|p| p.extension_name_as_c_str().map(CString::from).unwrap_or_default()).collect()
}

fn has(list: &[CString], name: &CStr) -> bool {
    list.iter().any(|n| n.as_c_str() == name)
}

#[cfg(windows)]
const WINDOW_EXTENSIONS: &[&CStr] = &[c"VK_KHR_win32_surface"];
#[cfg(target_os = "macos")]
const WINDOW_EXTENSIONS: &[&CStr] = &[c"VK_EXT_metal_surface"];
#[cfg(not(any(windows, target_os = "macos")))]
const WINDOW_EXTENSIONS: &[&CStr] = &[c"VK_KHR_xcb_surface", c"VK_KHR_xlib_surface", c"VK_KHR_wayland_surface"];

pub(crate) fn build_instance(request: &InstanceRequest<'_>) -> Result<BuiltInstance> {
    // SAFETY: loading the Vulkan loader library; nothing else runs during the load.
    let entry = unsafe { ash::Entry::load() }.map_err(|_| runtime("instance: vulkan_unavailable"))?;
    // SAFETY: plain queries through a loaded entry.
    let (available, layers, version) = unsafe {
        let available = names(&entry.enumerate_instance_extension_properties(None).unwrap_or_default());
        let layers: Vec<CString> = entry
            .enumerate_instance_layer_properties()
            .unwrap_or_default()
            .iter()
            .map(|l| l.layer_name_as_c_str().map(CString::from).unwrap_or_default())
            .collect();
        let version = entry.try_enumerate_instance_version().ok().flatten().unwrap_or(vk::API_VERSION_1_0);
        (available, layers, version)
    };
    let validation_available = has(&layers, VALIDATION_LAYER);
    let mut debug_utils_available = has(&available, ash::ext::debug_utils::NAME);
    if validation_available {
        // SAFETY: a plain query.
        let from_layer = unsafe { entry.enumerate_instance_extension_properties(Some(VALIDATION_LAYER)) }.unwrap_or_default();
        debug_utils_available |= has(&names(&from_layer), ash::ext::debug_utils::NAME);
    }
    let api_version = vk::make_api_version(0, 1, 4, 0);
    if version < api_version {
        return Err(runtime("instance: vulkan_version_1_4_unavailable"));
    }

    let mut extensions: Vec<CString> = request.extensions.clone();
    let use_debug_messenger = request.validation;
    if use_debug_messenger && debug_utils_available {
        extensions.push(ash::ext::debug_utils::NAME.into());
    }
    let layer_settings = request.validation && request.sync_validation;
    if layer_settings {
        extensions.push(c"VK_EXT_layer_settings".into());
    }
    let portability = has(&available, ash::khr::portability_enumeration::NAME);
    if portability {
        extensions.push(ash::khr::portability_enumeration::NAME.into());
    }
    if !request.headless {
        let surface = has(&available, ash::khr::surface::NAME);
        if surface {
            extensions.push(ash::khr::surface::NAME.into());
        }
        let mut window = false;
        for &name in WINDOW_EXTENSIONS {
            if has(&available, name) {
                extensions.push(name.into());
                window = true;
            }
        }
        if !surface || !window {
            return Err(runtime("instance: windowing_extensions_not_present"));
        }
    }
    let mut from_validation = Vec::new();
    if validation_available {
        // SAFETY: a plain query.
        from_validation = names(&unsafe { entry.enumerate_instance_extension_properties(Some(VALIDATION_LAYER)) }.unwrap_or_default());
    }
    let unsupported: Vec<String> =
        extensions.iter().filter(|e| !has(&available, e) && !has(&from_validation, e)).map(|e| e.to_string_lossy().into_owned()).collect();
    if !unsupported.is_empty() {
        return Err(runtime("instance: requested_extensions_not_present"));
    }
    let mut enabled_layers: Vec<&CStr> = Vec::new();
    if request.validation && validation_available {
        enabled_layers.push(VALIDATION_LAYER);
    }

    let app = vk::ApplicationInfo::default().application_name(request.app_name).engine_name(c"").api_version(api_version);
    let extension_pointers: Vec<*const i8> = extensions.iter().map(|e| e.as_ptr()).collect();
    let layer_pointers: Vec<*const i8> = enabled_layers.iter().map(|l| l.as_ptr()).collect();
    let severity = vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR;
    let kinds = vk::DebugUtilsMessageTypeFlagsEXT::GENERAL | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE;
    let mut messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(severity)
        .message_type(kinds)
        .pfn_user_callback(Some(on_message))
        .user_data(request.errors as *mut c_void);
    let sync_on: vk::Bool32 = vk::TRUE;
    let setting = vk::LayerSettingEXT {
        p_layer_name: VALIDATION_LAYER.as_ptr(),
        p_setting_name: c"validate_sync".as_ptr(),
        ty: vk::LayerSettingTypeEXT::BOOL32,
        value_count: 1,
        p_values: &sync_on as *const vk::Bool32 as *const c_void,
        ..Default::default()
    };
    let mut settings_info = vk::LayerSettingsCreateInfoEXT::default().settings(std::slice::from_ref(&setting));
    let mut info = vk::InstanceCreateInfo::default().application_info(&app).enabled_extension_names(&extension_pointers).enabled_layer_names(&layer_pointers);
    if portability {
        info = info.flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
    }
    if layer_settings {
        info = info.push_next(&mut settings_info);
    }
    if use_debug_messenger {
        info = info.push_next(&mut messenger_info);
    }
    // SAFETY: every pointer in `info` refers to locals alive for the call.
    let instance = unsafe { entry.create_instance(&info, None) }.map_err(|_| runtime("instance: failed_create_instance"))?;

    let mut debug_utils = None;
    let mut messenger = vk::DebugUtilsMessengerEXT::null();
    if use_debug_messenger {
        let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(severity)
            .message_type(kinds)
            .pfn_user_callback(Some(on_message))
            .user_data(request.errors as *mut c_void);
        // SAFETY: the instance is valid; the user data outlives the messenger (the owner destroys it first).
        match unsafe { loader.create_debug_utils_messenger(&info, None) } {
            Ok(m) => messenger = m,
            Err(_) => {
                // SAFETY: nothing else was created from the instance.
                unsafe { instance.destroy_instance(None) };
                return Err(runtime("instance: failed_create_debug_messenger"));
            }
        }
        debug_utils = Some(loader);
    }
    Ok(BuiltInstance { entry, instance, debug_utils, messenger, headless: request.headless })
}

impl BuiltInstance {
    // vkb::destroy_instance: the messenger, then the instance.
    pub(crate) unsafe fn destroy(&self) {
        // SAFETY: the caller destroyed every child object first.
        unsafe {
            if let Some(d) = &self.debug_utils {
                d.destroy_debug_utils_messenger(self.messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

// The features a selector requires, as the structures of vkb's extended features chain (in the order they were added).
#[derive(Clone, Copy, Default)]
pub(crate) struct RequiredFeatures {
    pub core: vk::PhysicalDeviceFeatures,
    pub vulkan13: Option<(bool, bool)>,
    pub vulkan14_push_descriptor: Option<bool>,
    pub interlock: Option<bool>,
}

pub(crate) struct Selector {
    pub surface: vk::SurfaceKHR,
    pub minimum_version: u32,
    pub features: RequiredFeatures,
    pub required_extensions: Vec<CString>,
}

pub(crate) struct PhysicalDevice {
    pub physical_device: vk::PhysicalDevice,
    pub properties: vk::PhysicalDeviceProperties,
    pub queue_families: Vec<vk::QueueFamilyProperties>,
    pub available_extensions: Vec<CString>,
    pub extensions_to_enable: Vec<CString>,
    pub features: RequiredFeatures,
    pub fault: bool,
    pub surface: vk::SurfaceKHR,
}

#[derive(Default)]
struct Queried {
    core: vk::PhysicalDeviceFeatures,
    dynamic_rendering: bool,
    synchronization2: bool,
    push_descriptor: bool,
    interlock: bool,
}

fn query(instance: &ash::Instance, device: vk::PhysicalDevice, wanted: &RequiredFeatures) -> Queried {
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut v14 = PhysicalDeviceVulkan14Features::default();
    let mut interlock = vk::PhysicalDeviceFragmentShaderInterlockFeaturesEXT::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default();
    let mut tail: *mut *mut c_void = &mut features2.p_next;
    // SAFETY: building a pNext chain of locals alive for the query.
    unsafe {
        if wanted.vulkan13.is_some() {
            *tail = &mut v13 as *mut _ as *mut c_void;
            tail = &mut v13.p_next;
        }
        if wanted.vulkan14_push_descriptor.is_some() {
            *tail = &mut v14 as *mut _ as *mut c_void;
            tail = &mut v14.p_next;
        }
        if wanted.interlock.is_some() {
            *tail = &mut interlock as *mut _ as *mut c_void;
        }
        instance.get_physical_device_features2(device, &mut features2);
    }
    Queried {
        core: features2.features,
        dynamic_rendering: v13.dynamic_rendering == vk::TRUE,
        synchronization2: v13.synchronization2 == vk::TRUE,
        push_descriptor: v14.push_descriptor == vk::TRUE,
        interlock: interlock.fragment_shader_pixel_interlock == vk::TRUE,
    }
}

fn core_supported(supported: &vk::PhysicalDeviceFeatures, wanted: &vk::PhysicalDeviceFeatures) -> bool {
    const COUNT: usize = std::mem::size_of::<vk::PhysicalDeviceFeatures>() / 4;
    // SAFETY: VkPhysicalDeviceFeatures is a plain array of VkBool32.
    let (s, w) = unsafe {
        (
            std::mem::transmute::<&vk::PhysicalDeviceFeatures, &[u32; COUNT]>(supported),
            std::mem::transmute::<&vk::PhysicalDeviceFeatures, &[u32; COUNT]>(wanted),
        )
    };
    s.iter().zip(w).all(|(s, w)| *w == 0 || *s != 0)
}

impl Selector {
    // PhysicalDeviceSelector::select: suitable devices, fully suitable (the preferred discrete type) before partly, the first.
    pub(crate) fn select(&self, built: &BuiltInstance) -> Result<PhysicalDevice> {
        let instance = &built.instance;
        let require_present = !built.headless;
        if require_present && self.surface == vk::SurfaceKHR::null() {
            return Err(runtime("physical device: no_surface_provided"));
        }
        let surface_loader = ash::khr::surface::Instance::new(&built.entry, instance);
        // SAFETY: plain queries on a valid instance.
        let devices = unsafe { instance.enumerate_physical_devices() }.map_err(|_| runtime("physical device: failed_enumerate_physical_devices"))?;
        if devices.is_empty() {
            return Err(runtime("physical device: no_physical_devices_found"));
        }
        let mut full = Vec::new();
        let mut partial = Vec::new();
        let mut reasons = Vec::new();
        for device in devices {
            // SAFETY: plain queries on an enumerated physical device.
            let (properties, queue_families, available) = unsafe {
                (
                    instance.get_physical_device_properties(device),
                    instance.get_physical_device_queue_family_properties(device),
                    names(&instance.enumerate_device_extension_properties(device).unwrap_or_default()),
                )
            };
            let name = properties.device_name_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if self.minimum_version > properties.api_version {
                reasons.push(format!("{name}: api version lower than required"));
                continue;
            }
            if require_present {
                let mut present = false;
                for i in 0..queue_families.len() as u32 {
                    // SAFETY: a plain query with a valid surface.
                    match unsafe { surface_loader.get_physical_device_surface_support(device, i, self.surface) } {
                        Ok(true) => {
                            present = true;
                            break;
                        }
                        Ok(false) => {}
                        Err(_) => break,
                    }
                }
                if !present {
                    reasons.push(format!("{name}: no queue capable of present operations"));
                    continue;
                }
            }
            if let Some(missing) = self.required_extensions.iter().find(|e| !has(&available, e)) {
                reasons.push(format!("{name}: device extension {} not supported", missing.to_string_lossy()));
                continue;
            }
            if require_present {
                // SAFETY: plain queries with a valid surface.
                let formats = unsafe { surface_loader.get_physical_device_surface_formats(device, self.surface) };
                let modes = unsafe { surface_loader.get_physical_device_surface_present_modes(device, self.surface) };
                if !matches!(&formats, Ok(f) if !f.is_empty()) || !matches!(&modes, Ok(m) if !m.is_empty()) {
                    reasons.push(format!("{name}: no surface formats or present modes"));
                    continue;
                }
            }
            let fully = properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU;
            let supported = query(instance, device, &self.features);
            let mut ok = core_supported(&supported.core, &self.features.core);
            if let Some((dynamic_rendering, synchronization2)) = self.features.vulkan13 {
                ok &= (!dynamic_rendering || supported.dynamic_rendering) && (!synchronization2 || supported.synchronization2);
            }
            if let Some(push) = self.features.vulkan14_push_descriptor {
                ok &= !push || supported.push_descriptor;
            }
            if let Some(interlock) = self.features.interlock {
                ok &= !interlock || supported.interlock;
            }
            if !ok {
                reasons.push(format!("{name}: required features not present"));
                continue;
            }
            let mut extensions_to_enable = self.required_extensions.clone();
            if has(&available, c"VK_KHR_portability_subset") {
                extensions_to_enable.push(c"VK_KHR_portability_subset".into());
            }
            extensions_to_enable.sort();
            let chosen = PhysicalDevice {
                physical_device: device,
                properties,
                queue_families,
                available_extensions: available,
                extensions_to_enable,
                features: self.features,
                fault: false,
                surface: self.surface,
            };
            if fully { full.push(chosen) } else { partial.push(chosen) }
        }
        full.extend(partial);
        full.into_iter().next().ok_or_else(|| runtime("physical device: no_suitable_device"))
    }
}

impl PhysicalDevice {
    pub(crate) fn is_extension_present(&self, name: &CStr) -> bool {
        has(&self.available_extensions, name)
    }

    // are_extension_features_present + enable_extension_if_present + enable_extension_features_if_present for
    // VkPhysicalDeviceFaultFeaturesEXT::deviceFault.
    pub(crate) fn enable_fault_if_present(&mut self, instance: &ash::Instance) -> bool {
        if !self.is_extension_present(ash::ext::device_fault::NAME) {
            return false;
        }
        let mut fault = vk::PhysicalDeviceFaultFeaturesEXT::default();
        let mut features2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut fault);
        // SAFETY: a plain query with a chain of locals.
        unsafe { instance.get_physical_device_features2(self.physical_device, &mut features2) };
        if fault.device_fault != vk::TRUE {
            return false;
        }
        let name = CString::from(ash::ext::device_fault::NAME);
        let at = self.extensions_to_enable.partition_point(|e| *e <= name);
        self.extensions_to_enable.insert(at, name);
        self.fault = true;
        true
    }
}

pub(crate) struct BuiltDevice {
    pub device: ash::Device,
    pub queue_families: Vec<vk::QueueFamilyProperties>,
}

// DeviceBuilder::build with no custom queues: one queue of priority 1 in every family; the features as
// VkPhysicalDeviceFeatures2 and the extended chain; VK_KHR_swapchain when there is a surface.
pub(crate) fn build_device(built: &BuiltInstance, physical: &PhysicalDevice) -> Result<BuiltDevice> {
    let priority = [1.0f32];
    let queues: Vec<vk::DeviceQueueCreateInfo> =
        (0..physical.queue_families.len() as u32).map(|i| vk::DeviceQueueCreateInfo::default().queue_family_index(i).queue_priorities(&priority)).collect();
    let mut extensions: Vec<CString> = physical.extensions_to_enable.clone();
    if physical.surface != vk::SurfaceKHR::null() {
        extensions.push(ash::khr::swapchain::NAME.into());
    }
    let extension_pointers: Vec<*const i8> = extensions.iter().map(|e| e.as_ptr()).collect();

    let f = &physical.features;
    let mut features2 = vk::PhysicalDeviceFeatures2::default().features(f.core);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut v14 = PhysicalDeviceVulkan14Features::default();
    let mut interlock = vk::PhysicalDeviceFragmentShaderInterlockFeaturesEXT::default();
    let mut fault = vk::PhysicalDeviceFaultFeaturesEXT::default().device_fault(true);
    let mut info = vk::DeviceCreateInfo::default().queue_create_infos(&queues).enabled_extension_names(&extension_pointers);
    // SAFETY: a pNext chain of locals alive for vkCreateDevice.
    let device = unsafe {
        let mut tail: *mut *mut c_void = &mut features2.p_next;
        if let Some((dynamic_rendering, synchronization2)) = f.vulkan13 {
            v13.dynamic_rendering = dynamic_rendering.into();
            v13.synchronization2 = synchronization2.into();
            *tail = &mut v13 as *mut _ as *mut c_void;
            tail = &mut v13.p_next;
        }
        if let Some(push) = f.vulkan14_push_descriptor {
            v14.push_descriptor = push.into();
            *tail = &mut v14 as *mut _ as *mut c_void;
            tail = &mut v14.p_next;
        }
        if let Some(pixel) = f.interlock {
            interlock.fragment_shader_pixel_interlock = pixel.into();
            *tail = &mut interlock as *mut _ as *mut c_void;
            tail = &mut interlock.p_next;
        }
        if physical.fault {
            *tail = &mut fault as *mut _ as *mut c_void;
        }
        info.p_next = &features2 as *const _ as *const c_void;
        built.instance.create_device(physical.physical_device, &info, None)
    }
    .map_err(|_| runtime("device: failed_create_device"))?;
    Ok(BuiltDevice { device, queue_families: physical.queue_families.clone() })
}

// Device::get_queue_index(QueueType::graphics): the first family with graphics.
pub(crate) fn graphics_queue(device: &BuiltDevice) -> Result<(vk::Queue, u32)> {
    let family =
        device.queue_families.iter().position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS)).ok_or_else(|| Error::Runtime("no graphics queue".into()))?
            as u32;
    // SAFETY: the family was created with one queue.
    let queue = unsafe { device.device.get_device_queue(family, 0) };
    Ok((queue, family))
}

pub(crate) fn allocator(built: &BuiltInstance, device: &ash::Device, physical: vk::PhysicalDevice) -> Result<vk_mem::Allocator> {
    let mut info = vk_mem::AllocatorCreateInfo::new(&built.instance, device, physical);
    info.vulkan_api_version = vk::make_api_version(0, 1, 4, 0);
    // SAFETY: instance, device and physical device outlive the allocator (the owner drops it first).
    unsafe { vk_mem::Allocator::new(info) }.map_err(|r| runtime(format!("allocator: {}", r.as_raw())))
}
