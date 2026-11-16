mod bootstrap;
pub mod device;
pub mod error;
pub mod gpu_device;
pub mod headless_context;
pub mod native_renderer;
pub mod texture_file;

pub use ash;
pub use ash::vk;
pub use device::{Device, DeviceOptions, FrameContext, Window};
pub use error::{Error, Result};
pub use gpu_device::GpuDevice;
pub use headless_context::HeadlessContext;
pub use native_renderer::{GpuVertex, NativeOutput, NativeRenderer};
pub use vk_mem;

// The directory build.rs compiled Shaders/Native.{vert,frag} into (the C++ build's CLOCK_SHADERS).
pub const SHADER_DIRECTORY: &str = concat!(env!("OUT_DIR"), "/shaders");
pub const NATIVE_VERT_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders/Native.vert.spv"));
pub const NATIVE_FRAG_SPV: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders/Native.frag.spv"));
