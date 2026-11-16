// tests/render/DeviceTest.cpp and tests/HeadlessTest.cpp.

mod support;

use render::{HeadlessContext, vk};

#[test]
fn headless_device() {
    let Some(device) = support::device() else { return };
    assert!(device.device().handle() != vk::Device::null());
    assert!(device.queue() != vk::Queue::null());
    assert!(device.begin_frame().unwrap().is_none());
    assert!(device.sample_counts().contains(vk::SampleCountFlags::TYPE_1));
    assert_eq!(device.validation_errors(), 0);
}

#[test]
fn headless_context() {
    let context = match HeadlessContext::new(true) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipped: no Vulkan device with fragment shader interlock ({e})");
            return;
        }
    };
    let gpu = context.gpu();
    assert!(gpu.device.handle() != vk::Device::null() && gpu.queue != vk::Queue::null());

    let mut interlock = vk::PhysicalDeviceFragmentShaderInterlockFeaturesEXT::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut interlock);
    // SAFETY: a plain query on the context's physical device.
    unsafe { context.instance().get_physical_device_features2(gpu.physical_device, &mut features) };
    let fragment_stores = features.features.fragment_stores_and_atomics;
    assert_eq!(interlock.fragment_shader_pixel_interlock, vk::TRUE);
    assert_eq!(fragment_stores, vk::TRUE);

    // SAFETY: plain queries.
    let rgba = unsafe { context.instance().get_physical_device_format_properties(gpu.physical_device, vk::Format::R8G8B8A8_UINT) };
    assert!(rgba.optimal_tiling_features.contains(vk::FormatFeatureFlags::STORAGE_IMAGE));
    let r32 = unsafe { context.instance().get_physical_device_format_properties(gpu.physical_device, vk::Format::R32_UINT) };
    assert!(r32.optimal_tiling_features.contains(vk::FormatFeatureFlags::STORAGE_IMAGE));
    assert_eq!(context.validation_errors(), 0);
}

#[test]
fn shaders_are_built() {
    assert_eq!(&render::NATIVE_VERT_SPV[..4], &0x0723_0203u32.to_le_bytes());
    assert_eq!(&render::NATIVE_FRAG_SPV[..4], &0x0723_0203u32.to_le_bytes());
    let dir = support::shaders();
    assert_eq!(std::fs::read(dir.join("Native.vert.spv")).unwrap(), render::NATIVE_VERT_SPV);
    assert_eq!(std::fs::read(dir.join("Native.frag.spv")).unwrap(), render::NATIVE_FRAG_SPV);
}
