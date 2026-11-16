use ash::vk;

#[derive(Clone, Copy)]
pub struct GpuDevice<'a> {
    pub device: &'a ash::Device,
    pub physical_device: vk::PhysicalDevice,
    pub allocator: &'a vk_mem::Allocator,
    pub queue: vk::Queue,
    pub queue_family: u32,
}
