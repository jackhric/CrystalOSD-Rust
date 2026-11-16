use std::collections::BTreeMap;
use std::path::Path;

use ash::vk;
use vk_mem::Alloc;

use scene::font::Font;
use scene::frame::{
    BlendOp, CoordinateKind, DepthTest, Frame, GLYPH_LAYOUT_TEXTURE, GLYPH_TEXTURE, GlyphCache, Pass, PassTopology, Sampling, SourceKind, TargetName,
    TextureSet,
};

use crate::device::Device;
use crate::error::{Error, Result, check, runtime};
use crate::texture_file;

mod sharp_glyphs;

// The resolution the frame is drawn at (its scale is height / 448: the NTSC frame of two 224-line fields)
// and the MSAA sample count, which replaces the OSD's edge antialiasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeOutput {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
}

impl Default for NativeOutput {
    fn default() -> Self {
        Self { width: 640, height: 448, samples: 1 }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub q: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

const COLOUR_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PushState {
    scene: [f32; 4],
    source: [f32; 4],
    region: [f32; 4],
    mode: [i32; 4],
    shade: [i32; 4],
    extent: [f32; 4],
}

impl PushState {
    fn bytes(&self) -> [u8; 96] {
        // SAFETY: PushState is 96 bytes of f32 and i32, no padding.
        unsafe { std::mem::transmute(*self) }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_stage: vk::PipelineStageFlags2,
    dst_access: vk::AccessFlags2,
) {
    let b = [vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(from)
        .new_layout(to)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 })];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&b);
    // SAFETY: `cmd` is recording on `device`.
    unsafe { device.cmd_pipeline_barrier2(cmd, &dependency) };
}

fn blend_state(op: BlendOp) -> vk::PipelineColorBlendAttachmentState {
    let mut s = vk::PipelineColorBlendAttachmentState { color_write_mask: vk::ColorComponentFlags::RGBA, ..Default::default() };
    if op == BlendOp::Opaque {
        return s;
    }
    // The destination's alpha becomes the source's, as on the GS (blending touches the colour only).
    s.blend_enable = vk::TRUE;
    s.src_alpha_blend_factor = vk::BlendFactor::ONE;
    s.dst_alpha_blend_factor = vk::BlendFactor::ZERO;
    s.alpha_blend_op = vk::BlendOp::ADD;
    s.color_blend_op = vk::BlendOp::ADD;
    match op {
        BlendOp::Add => {
            s.src_color_blend_factor = vk::BlendFactor::ONE;
            s.dst_color_blend_factor = vk::BlendFactor::ONE;
        }
        BlendOp::Subtract => {
            s.src_color_blend_factor = vk::BlendFactor::ONE;
            s.dst_color_blend_factor = vk::BlendFactor::ONE;
            s.color_blend_op = vk::BlendOp::REVERSE_SUBTRACT;
        }
        BlendOp::AlphaOver => {
            s.src_color_blend_factor = vk::BlendFactor::SRC_ALPHA;
            s.dst_color_blend_factor = vk::BlendFactor::ONE_MINUS_SRC_ALPHA;
        }
        BlendOp::FixedOver => {
            s.src_color_blend_factor = vk::BlendFactor::CONSTANT_ALPHA;
            s.dst_color_blend_factor = vk::BlendFactor::ONE_MINUS_CONSTANT_ALPHA;
        }
        BlendOp::FixedAdd => {
            s.src_color_blend_factor = vk::BlendFactor::CONSTANT_ALPHA;
            s.dst_color_blend_factor = vk::BlendFactor::ONE;
        }
        BlendOp::AddDestinationAlpha => {
            s.src_color_blend_factor = vk::BlendFactor::DST_ALPHA;
            s.dst_color_blend_factor = vk::BlendFactor::ONE;
        }
        BlendOp::SubtractFixed => {
            s.src_color_blend_factor = vk::BlendFactor::CONSTANT_ALPHA;
            s.dst_color_blend_factor = vk::BlendFactor::ONE;
            s.color_blend_op = vk::BlendOp::REVERSE_SUBTRACT;
        }
        BlendOp::Opaque => {}
    }
    s
}

// Add and Subtract leave the shader as Cs * As: the attachment clamps a factor to 1, the shader does not,
// and the sum or difference then clamps as the GS's does (both terms are non-negative).
fn premultiplied(op: BlendOp) -> bool {
    op == BlendOp::Add || op == BlendOp::Subtract
}

fn compare_of(test: DepthTest) -> vk::CompareOp {
    match test {
        DepthTest::Always => vk::CompareOp::ALWAYS,
        DepthTest::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
        DepthTest::Greater => vk::CompareOp::GREATER,
    }
}

// Scene vertices as the GPU draws them, all as triangles; z / 2^24 the depth. A sprite's two corners become two
// triangles. A line becomes the band the GS covers: one pixel across its minor axis (a row for an x-major line,
// a column for a y-major one), so consecutive segments of a strip meet edge to edge. Wide lines (square across
// the line) overlapped at the joints and covered two GS pixels across steep segments at the window's scale, and
// the trail's additive blend showed both as bright patches.
fn append_vertices(out: &mut Vec<GpuVertex>, pass: &Pass, depth_bits: i32) {
    let depth_mask: u32 = if depth_bits == 24 { 0xffffff } else { 0xffffffff };
    let mut put = |v: &scene::frame::Vertex, x: f32, y: f32, u: f32, t: f32, z: u32, q: f32| {
        out.push(GpuVertex { x, y, z: (z & depth_mask) as f32 / 16777216.0, u, v: t, q, r: v.r, g: v.g, b: v.b, a: v.a });
    };
    let vertices = &pass.vertices;
    if pass.topology == PassTopology::Lines {
        let mut i = 0;
        while i + 1 < vertices.len() {
            let a = &vertices[i];
            let b = &vertices[i + 1];
            let x_major = (b.x - a.x).abs() >= (b.y - a.y).abs();
            let (ox, oy) = if x_major { (0.0f32, 0.5f32) } else { (0.5, 0.0) };
            put(a, a.x - ox, a.y - oy, a.u, a.v, a.z, a.q);
            put(a, a.x + ox, a.y + oy, a.u, a.v, a.z, a.q);
            put(b, b.x - ox, b.y - oy, b.u, b.v, b.z, b.q);
            put(a, a.x + ox, a.y + oy, a.u, a.v, a.z, a.q);
            put(b, b.x + ox, b.y + oy, b.u, b.v, b.z, b.q);
            put(b, b.x - ox, b.y - oy, b.u, b.v, b.z, b.q);
            i += 2;
        }
        return;
    }
    if pass.topology != PassTopology::Sprites {
        for v in vertices {
            put(v, v.x, v.y, v.u, v.v, v.z, v.q);
        }
        return;
    }
    let mut i = 0;
    while i + 1 < vertices.len() {
        let a = &vertices[i];
        let b = &vertices[i + 1];
        put(b, a.x, a.y, a.u, a.v, b.z, b.q);
        put(b, b.x, a.y, b.u, a.v, b.z, b.q);
        put(b, a.x, b.y, a.u, b.v, b.z, b.q);
        put(b, b.x, a.y, b.u, a.v, b.z, b.q);
        put(b, b.x, b.y, b.u, b.v, b.z, b.q);
        put(b, a.x, b.y, a.u, b.v, b.z, b.q);
        i += 2;
    }
}

// The clock's textures at their 640 x 224 addresses (clock_frame.mjs stateWriters; parity/FromScene clockLayout):
// TEXCFLOW, TEXCKABE, TEXCBUMP, TEXCBINV, TEXCSMOK, TEXCREFA, TEXCNAVI, TEXCBLUR, TEXCSTSL, TEXCMARU.
const TARGET_TEXTURE_WIDTH: f32 = 1024.0;
const TARGET_TEXTURE_HEIGHT: f32 = 256.0;
const CLOCK_SET: i32 = TextureSet::Clock as i32;

const CLOCK_TEXTURES: [&str; 10] = [
    "tbp-2bc0-64x64.png",
    "tbp-2c00-128x128.png",
    "tbp-2d00-64x64.png",
    "tbp-2d40-64x64.png",
    "tbp-2d80-64x64.png",
    "tbp-2dc0-64x64.png",
    "tbp-2e00-64x64.png",
    "tbp-2e40-64x64.png",
    "tbp-2e80-64x64.png",
    "tbp-2ec0-64x64.png",
];

const TARGETS: usize = 5;

#[derive(Default)]
struct Image {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_mem::Allocation>,
    aspect: vk::ImageAspectFlags,
    width: u32,
    height: u32,
    layout: vk::ImageLayout,
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
}

impl Image {
    fn exists(&self) -> bool {
        self.image != vk::Image::null()
    }
}

#[derive(Default)]
struct Target {
    resolved: Image,
    multisampled: Image,
}

struct Buffer {
    buffer: vk::Buffer,
    allocation: Option<vk_mem::Allocation>,
    mapped: *mut u8,
    size: vk::DeviceSize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self { buffer: vk::Buffer::null(), allocation: None, mapped: std::ptr::null_mut(), size: 0 }
    }
}

struct Texture {
    image: Image,
    largest_alpha: u8,
    logical: [f32; 2],
}

type PipelineKey = (i32, i32, bool, u32);
type TextureKey = (i32, i32);

// Draws a scene::Frame with an ordinary graphics pipeline: float blending, no GS rules.
// Units (scene/frame.rs): colour bytes, alpha 0x80 = 1.0. Targets store alpha / 128, so a target read back
// or sampled gives the alpha byte the OSD would have; read_target converts at that boundary.
pub struct NativeRenderer<'d> {
    device: &'d Device,
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    samplers: [vk::Sampler; 4],
    pipelines: BTreeMap<PipelineKey, vk::Pipeline>,
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    textures: BTreeMap<TextureKey, Texture>,
    blank: Image,
    targets: [Target; TARGETS],
    alpha_bound: [u32; TARGETS],
    depth: Image,
    output: NativeOutput,
    vertices: [Buffer; 3],
    slot: usize,
    staging: Vec<GpuVertex>,
    ranges: Vec<(u32, u32)>,
    glyphs: GlyphCache,
    glyph_layouts: Vec<GlyphCache>,
    sharp: Option<sharp_glyphs::SharpGlyphs>,
}

fn read_spirv(path: &Path) -> Result<Vec<u32>> {
    let bytes = std::fs::read(path).map_err(|_| runtime(format!("cannot read {}", path.display())))?;
    ash::util::read_spv(&mut std::io::Cursor::new(bytes)).map_err(|_| runtime(format!("cannot read {}", path.display())))
}

impl<'d> NativeRenderer<'d> {
    // Native.vert.spv and Native.frag.spv from `shader_directory` (crate::SHADER_DIRECTORY holds the build's).
    pub fn new(device: &'d Device, shader_directory: &Path) -> Result<NativeRenderer<'d>> {
        let vertex = read_spirv(&shader_directory.join("Native.vert.spv"))?;
        let fragment = read_spirv(&shader_directory.join("Native.frag.spv"))?;
        Self::with_spirv(device, &vertex, &fragment)
    }

    pub fn with_spirv(device: &'d Device, vertex_code: &[u32], fragment_code: &[u32]) -> Result<NativeRenderer<'d>> {
        let mut r = NativeRenderer {
            device,
            vertex: vk::ShaderModule::null(),
            fragment: vk::ShaderModule::null(),
            set_layout: vk::DescriptorSetLayout::null(),
            layout: vk::PipelineLayout::null(),
            samplers: [vk::Sampler::null(); 4],
            pipelines: BTreeMap::new(),
            pool: vk::CommandPool::null(),
            commands: vk::CommandBuffer::null(),
            textures: BTreeMap::new(),
            blank: Image::default(),
            targets: Default::default(),
            alpha_bound: [0; TARGETS],
            depth: Image::default(),
            output: NativeOutput { width: 0, height: 0, samples: 0 },
            vertices: Default::default(),
            slot: 0,
            staging: Vec::new(),
            ranges: Vec::new(),
            glyphs: GlyphCache::default(),
            glyph_layouts: Vec::new(),
            sharp: None,
        };
        let d = device.device();
        // SAFETY: creating objects on a valid device; Drop destroys them (null handles are ignored).
        unsafe {
            r.vertex = check(d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(vertex_code), None), "shader module")?;
            r.fragment = check(d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(fragment_code), None), "shader module")?;

            let binding = [vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
            let set = vk::DescriptorSetLayoutCreateInfo::default().flags(vk::DescriptorSetLayoutCreateFlags::PUSH_DESCRIPTOR_KHR).bindings(&binding);
            r.set_layout = check(d.create_descriptor_set_layout(&set, None), "set layout")?;
            let push = [vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: std::mem::size_of::<PushState>() as u32,
            }];
            let layouts = [r.set_layout];
            let layout = vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts).push_constant_ranges(&push);
            r.layout = check(d.create_pipeline_layout(&layout, None), "pipeline layout")?;

            for i in 0..r.samplers.len() {
                let filter = if i & 1 != 0 { vk::Filter::LINEAR } else { vk::Filter::NEAREST };
                let address = if i & 2 != 0 { vk::SamplerAddressMode::CLAMP_TO_EDGE } else { vk::SamplerAddressMode::REPEAT };
                let s = vk::SamplerCreateInfo::default()
                    .mag_filter(filter)
                    .min_filter(filter)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(address)
                    .address_mode_v(address)
                    .address_mode_w(address)
                    .max_lod(0.0);
                r.samplers[i] = check(d.create_sampler(&s, None), "sampler")?;
            }

            let pool = vk::CommandPoolCreateInfo::default().flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER).queue_family_index(device.queue_family());
            r.pool = check(d.create_command_pool(&pool, None), "command pool")?;
            let alloc = vk::CommandBufferAllocateInfo::default().command_pool(r.pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1);
            r.commands = check(d.allocate_command_buffers(&alloc), "command buffer")?[0];
        }

        r.blank = r.create_image(
            1,
            1,
            COLOUR_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        let cmd = r.begin_submit()?;
        let mut blank = std::mem::take(&mut r.blank);
        r.transition(cmd, &mut blank, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
        let white = vk::ClearColorValue { float32: [1.0, 1.0, 1.0, 1.0] };
        // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
        unsafe { d.cmd_clear_color_image(cmd, blank.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &white, &[colour_range()]) };
        r.transition(
            cmd,
            &mut blank,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
        r.blank = blank;
        r.end_submit(cmd)?;
        Ok(r)
    }

    fn create_image(
        &self,
        width: u32,
        height: u32,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        samples: vk::SampleCountFlags,
        aspect: vk::ImageAspectFlags,
    ) -> Result<Image> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D { width, height, depth: 1 })
            .mip_levels(1)
            .array_layers(1)
            .samples(samples)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let alloc = vk_mem::AllocationCreateInfo { usage: vk_mem::MemoryUsage::AutoPreferDevice, ..Default::default() };
        // SAFETY: a valid allocator and create info.
        let (image, allocation) = check(unsafe { self.device.allocator().create_image(&info, &alloc) }, "image")?;
        let view = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 });
        // SAFETY: the image was just created on this device.
        let view = check(unsafe { self.device.device().create_image_view(&view, None) }, "image view")?;
        Ok(Image {
            image,
            view,
            allocation: Some(allocation),
            aspect,
            width,
            height,
            layout: vk::ImageLayout::UNDEFINED,
            stage: vk::PipelineStageFlags2::NONE,
            access: vk::AccessFlags2::empty(),
        })
    }

    fn destroy_image(&self, image: &mut Image) {
        // SAFETY: the device is idle where images are destroyed (callers wait first).
        unsafe {
            if image.view != vk::ImageView::null() {
                self.device.device().destroy_image_view(image.view, None);
            }
            if let Some(mut allocation) = image.allocation.take() {
                self.device.allocator().destroy_image(image.image, &mut allocation);
            }
        }
        *image = Image::default();
    }

    fn create_buffer(&self, size: vk::DeviceSize, usage: vk::BufferUsageFlags) -> Result<Buffer> {
        let info = vk::BufferCreateInfo::default().size(size).usage(usage);
        let host = if usage.contains(vk::BufferUsageFlags::TRANSFER_DST) {
            vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM
        } else {
            vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE
        };
        let alloc =
            vk_mem::AllocationCreateInfo { usage: vk_mem::MemoryUsage::Auto, flags: vk_mem::AllocationCreateFlags::MAPPED | host, ..Default::default() };
        // SAFETY: a valid allocator and create info.
        let (buffer, allocation) = check(unsafe { self.device.allocator().create_buffer(&info, &alloc) }, "buffer")?;
        let mapped = self.device.allocator().get_allocation_info(&allocation).mapped_data as *mut u8;
        Ok(Buffer { buffer, allocation: Some(allocation), mapped, size })
    }

    fn destroy_buffer(&self, buffer: &mut Buffer) {
        if let Some(mut allocation) = buffer.allocation.take() {
            // SAFETY: the buffer is no longer in use (callers wait or the buffer was never submitted).
            unsafe { self.device.allocator().destroy_buffer(buffer.buffer, &mut allocation) };
        }
        *buffer = Buffer::default();
    }

    fn destroy_targets(&mut self) {
        let mut targets = std::mem::take(&mut self.targets);
        for t in &mut targets {
            self.destroy_image(&mut t.resolved);
            self.destroy_image(&mut t.multisampled);
        }
        let mut depth = std::mem::take(&mut self.depth);
        self.destroy_image(&mut depth);
    }

    // submit(record): the first half; end_submit is the second (vkEndCommandBuffer, vkQueueSubmit2, vkQueueWaitIdle).
    fn begin_submit(&self) -> Result<vk::CommandBuffer> {
        let d = self.device.device();
        // SAFETY: the command buffer's last submission completed (end_submit waits for the queue).
        unsafe {
            check(d.reset_command_buffer(self.commands, vk::CommandBufferResetFlags::empty()), "reset command buffer")?;
            let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            check(d.begin_command_buffer(self.commands, &begin), "begin command buffer")?;
        }
        Ok(self.commands)
    }

    fn end_submit(&self, cmd: vk::CommandBuffer) -> Result<()> {
        let d = self.device.device();
        // SAFETY: `cmd` is recording; the queue belongs to the device.
        unsafe {
            check(d.end_command_buffer(cmd), "end command buffer")?;
            let infos = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
            let info = vk::SubmitInfo2::default().command_buffer_infos(&infos);
            self.device.verify(d.queue_submit2(self.device.queue(), &[info], vk::Fence::null()), "submit")?;
            self.device.verify(d.queue_wait_idle(self.device.queue()), "wait")?;
        }
        Ok(())
    }

    fn transition(&self, cmd: vk::CommandBuffer, image: &mut Image, layout: vk::ImageLayout, stage: vk::PipelineStageFlags2, access: vk::AccessFlags2) {
        barrier(self.device.device(), cmd, image.image, image.aspect, image.layout, layout, image.stage, image.access, stage, access);
        image.layout = layout;
        image.stage = stage;
        image.access = access;
    }

    pub fn set_texture(&mut self, number: i32, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
        self.set_texture_in(TextureSet::Clock, number, width, height, rgba)
    }

    pub fn set_texture_in(&mut self, set: TextureSet, number: i32, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
        let key = (set as i32, number);
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(runtime(format!("texture {number}: wrong size")));
        }
        // SAFETY: a valid device.
        unsafe { self.device.device().device_wait_idle() }.ok();
        if let Some(mut found) = self.textures.remove(&key) {
            self.destroy_image(&mut found.image);
        }
        let mut image = self.create_image(
            width,
            height,
            COLOUR_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        let largest_alpha = rgba.iter().skip(3).step_by(4).copied().max().unwrap_or(0);
        let mut staging = self.create_buffer(rgba.len() as vk::DeviceSize, vk::BufferUsageFlags::TRANSFER_SRC)?;
        // SAFETY: the buffer is mapped and at least rgba.len() bytes.
        unsafe { std::ptr::copy_nonoverlapping(rgba.as_ptr(), staging.mapped, rgba.len()) };
        check(self.device.allocator().flush_allocation(staging.allocation.as_ref().expect("an allocation"), 0, vk::WHOLE_SIZE), "flush")?;
        let cmd = self.begin_submit()?;
        self.transition(cmd, &mut image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
        let copy = vk::BufferImageCopy {
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
            image_extent: vk::Extent3D { width, height, depth: 1 },
            ..Default::default()
        };
        // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
        unsafe { self.device.device().cmd_copy_buffer_to_image(cmd, staging.buffer, image.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[copy]) };
        self.transition(
            cmd,
            &mut image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
        let submitted = self.end_submit(cmd);
        self.destroy_buffer(&mut staging);
        if let Err(e) = submitted {
            self.destroy_image(&mut image);
            return Err(e);
        }
        self.textures.insert(key, Texture { image, largest_alpha, logical: [width as f32, height as f32] });
        Ok(())
    }

    // facts/text.md sections 1 and 2: 4-bit pictures through the block's colour table. The GS looks a texel's colour up
    // before it filters, so the table is applied here, once, and the pass samples the colours.
    pub fn set_glyph_cache(&mut self, font: &Font, cache: &GlyphCache) -> Result<()> {
        if *cache == self.glyphs && self.textures.contains_key(&(CLOCK_SET, GLYPH_TEXTURE)) {
            return Ok(());
        }
        if self.set_sharp_glyph_cache(font, cache)? {
            return Ok(());
        }
        let image = scene::text::glyph_cache_image(font, cache).map_err(|e| crate::error::Error::Runtime(e.0))?;
        self.set_texture(GLYPH_TEXTURE, 1u32 << cache.log_width, 1u32 << cache.log_height, &image)?;
        self.glyphs = cache.clone();
        Ok(())
    }

    // The layouts a frame's cache passed through after its first (Frame::glyph_layouts), each its own texture.
    pub fn set_glyph_layouts(&mut self, font: &Font, layouts: &[GlyphCache]) -> Result<()> {
        if layouts == self.glyph_layouts.as_slice() {
            return Ok(());
        }
        for (k, cache) in layouts.iter().enumerate() {
            let image = scene::text::glyph_cache_image(font, cache).map_err(|e| crate::error::Error::Runtime(e.0))?;
            self.set_texture(GLYPH_LAYOUT_TEXTURE + k as i32, 1u32 << cache.log_width, 1u32 << cache.log_height, &image)?;
        }
        self.glyph_layouts = layouts.to_vec();
        Ok(())
    }

    // The ten clock textures (References/textures, written by References/scripts/extract_textures.mjs and
    // checked against the ROM by extract_rom_textures.mjs), texture n at its 640 x 224 address.
    pub fn load_clock_textures(&mut self, directory: &Path) -> Result<()> {
        for (n, name) in CLOCK_TEXTURES.iter().enumerate() {
            let path = directory.join(name);
            let (w, h, pixels) = texture_file::load_rgba(&path).ok_or_else(|| runtime(format!("cannot read the clock texture {}", path.display())))?;
            self.set_texture(n as i32, w, h, &pixels)?;
        }
        Ok(())
    }

    // The opening's textures (References/textures/opening, written by References/scripts/extract_opening_textures.mjs):
    // tex<number>-<width>x<height>.png, RGBA8 with TEXA's alpha, drawn by frames of scene::TextureSet::Opening.
    pub fn load_opening_textures(&mut self, directory: &Path) -> Result<()> {
        let mut loaded = 0usize;
        let entries = std::fs::read_dir(directory).map_err(|e| runtime(format!("{}: {e}", directory.display())))?;
        for entry in entries {
            let entry = entry.map_err(|e| runtime(format!("{}: {e}", directory.display())))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((number, width, height)) = scan_texture_name(&name) else { continue };
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            let (w, h, pixels) = texture_file::load_rgba(&path).ok_or_else(|| runtime(format!("cannot read the opening texture {}", path.display())))?;
            if w as i32 != width || h as i32 != height {
                return Err(runtime(format!("the opening texture {name} is {w}x{h}")));
            }
            self.set_texture_in(TextureSet::Opening, number, w, h, &pixels)?;
            loaded += 1;
        }
        if loaded == 0 {
            return Err(runtime(format!("no opening textures in {}", directory.display())));
        }
        Ok(())
    }

    pub fn configure(&mut self, output: &NativeOutput) -> Result<()> {
        if *output == self.output {
            return Ok(());
        }
        if output.width == 0 || output.height == 0 {
            return Err(runtime("an empty output"));
        }
        if !self.device.sample_counts().intersects(vk::SampleCountFlags::from_raw(output.samples)) {
            return Err(runtime(format!("MSAA {}x is not supported", output.samples)));
        }
        // SAFETY: a valid device.
        unsafe { self.device.device().device_wait_idle() }.ok();
        self.destroy_targets();
        self.output = *output;
        if self.sharp.is_some() {
            self.glyphs = GlyphCache::default();
        }
        self.alpha_bound = [0; TARGETS];
        let samples = vk::SampleCountFlags::from_raw(output.samples);
        for i in 0..=TargetName::Work as usize {
            let t = self.create_target()?;
            self.targets[i] = t;
        }
        self.depth = self.create_image(
            output.width,
            output.height,
            DEPTH_FORMAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
            samples,
            vk::ImageAspectFlags::DEPTH,
        )?;
        let cmd = self.begin_submit()?;
        let black = vk::ClearColorValue::default();
        let mut targets = std::mem::take(&mut self.targets);
        for t in &mut targets {
            for image in [&mut t.resolved, &mut t.multisampled] {
                if !image.exists() {
                    continue;
                }
                self.transition(cmd, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
                // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
                unsafe { self.device.device().cmd_clear_color_image(cmd, image.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &black, &[colour_range()]) };
            }
        }
        self.targets = targets;
        let mut depth = std::mem::take(&mut self.depth);
        self.transition(cmd, &mut depth, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
        let zero = vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 };
        let range =
            vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::DEPTH, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
        // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
        unsafe { self.device.device().cmd_clear_depth_stencil_image(cmd, depth.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &zero, &[range]) };
        self.depth = depth;
        self.end_submit(cmd)
    }

    pub fn output(&self) -> &NativeOutput {
        &self.output
    }

    fn create_target(&self) -> Result<Target> {
        let resolved = self.create_image(
            self.output.width,
            self.output.height,
            COLOUR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageAspectFlags::COLOR,
        )?;
        let mut multisampled = Image::default();
        if self.output.samples > 1 {
            multisampled = self.create_image(
                self.output.width,
                self.output.height,
                COLOUR_FORMAT,
                vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST,
                vk::SampleCountFlags::from_raw(self.output.samples),
                vk::ImageAspectFlags::COLOR,
            )?;
        }
        Ok(Target { resolved, multisampled })
    }

    // The opening's Store and Extra targets exist only for frames that use them; the clock keeps its three.
    fn ensure_targets(&mut self, cmd: vk::CommandBuffer, frame: &Frame) -> Result<()> {
        let black = vk::ClearColorValue::default();
        for pass in &frame.passes {
            if pass.vertices.is_empty() {
                continue;
            }
            let mut names = vec![pass.target];
            if pass.material.source == SourceKind::Target {
                names.push(pass.material.source_target);
            }
            for name in names {
                if self.targets[name as usize].resolved.exists() {
                    continue;
                }
                let mut t = self.create_target()?;
                for image in [&mut t.resolved, &mut t.multisampled] {
                    if !image.exists() {
                        continue;
                    }
                    self.transition(cmd, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
                    // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
                    unsafe { self.device.device().cmd_clear_color_image(cmd, image.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &black, &[colour_range()]) };
                }
                self.targets[name as usize] = t;
            }
        }
        Ok(())
    }

    fn pipeline(&mut self, pass: &Pass, blend_op: BlendOp) -> Result<vk::Pipeline> {
        let m = &pass.material;
        // AA1 writes no depth on an edge pixel, and every pixel of an AA1 line is an edge pixel (shaders/GsParity.frag).
        let depth_write = m.depth_write && !(pass.topology == PassTopology::Lines && pass.edge_smoothing);
        let key: PipelineKey = (blend_op as i32, m.depth_test as i32, depth_write, self.output.samples);
        if let Some(&p) = self.pipelines.get(&key) {
            return Ok(p);
        }

        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(self.vertex).name(c"main"),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(self.fragment).name(c"main"),
        ];
        let binding =
            [vk::VertexInputBindingDescription { binding: 0, stride: std::mem::size_of::<GpuVertex>() as u32, input_rate: vk::VertexInputRate::VERTEX }];
        let attributes = [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::offset_of!(GpuVertex, x) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32B32_SFLOAT,
                offset: std::mem::offset_of!(GpuVertex, u) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R8G8B8A8_UINT,
                offset: std::mem::offset_of!(GpuVertex, r) as u32,
            },
        ];
        let input = vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(&binding).vertex_attribute_descriptions(&attributes);
        let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::from_raw(self.output.samples));
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(depth_write)
            .depth_compare_op(compare_of(m.depth_test));
        let attachment = [blend_state(blend_op)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachment);
        let dynamics = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR, vk::DynamicState::BLEND_CONSTANTS];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamics);
        let formats = [COLOUR_FORMAT];
        let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats).depth_attachment_format(DEPTH_FORMAT);
        let info = vk::GraphicsPipelineCreateInfo::default()
            .push_next(&mut rendering)
            .stages(&stages)
            .vertex_input_state(&input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(self.layout);
        // SAFETY: every pointer in `info` refers to locals alive for the call.
        let p = unsafe { self.device.device().create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
            .map_err(|(_, r)| runtime(format!("pipeline: {}", r.as_raw())))?[0];
        self.pipelines.insert(key, p);
        Ok(p)
    }

    fn begin_rendering(&mut self, cmd: vk::CommandBuffer, name: TargetName) {
        let mut t = std::mem::take(&mut self.targets[name as usize]);
        let multisampled = t.multisampled.exists();
        let colour_access = vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
        if multisampled {
            self.transition(
                cmd,
                &mut t.multisampled,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                colour_access,
            );
        }
        self.transition(cmd, &mut t.resolved, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, colour_access);
        let mut depth_image = std::mem::take(&mut self.depth);
        self.transition(
            cmd,
            &mut depth_image,
            vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );

        let mut colour = vk::RenderingAttachmentInfo::default()
            .image_view(if multisampled { t.multisampled.view } else { t.resolved.view })
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        if multisampled {
            colour = colour
                .resolve_mode(vk::ResolveModeFlags::AVERAGE)
                .resolve_image_view(t.resolved.view)
                .resolve_image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        }
        let depth = vk::RenderingAttachmentInfo::default()
            .image_view(depth_image.view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        let colours = [colour];
        let extent = vk::Extent2D { width: self.output.width, height: self.output.height };
        let info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
            .layer_count(1)
            .color_attachments(&colours)
            .depth_attachment(&depth);
        let d = self.device.device();
        // SAFETY: `cmd` is recording outside a rendering scope; the attachments are in the layouts given.
        unsafe {
            d.cmd_begin_rendering(cmd, &info);
            let viewport = vk::Viewport { x: 0.0, y: 0.0, width: self.output.width as f32, height: self.output.height as f32, min_depth: 0.0, max_depth: 1.0 };
            d.cmd_set_viewport(cmd, 0, &[viewport]);
            d.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);
        }
        self.depth = depth_image;
        self.targets[name as usize] = t;
    }

    fn written_alpha_with(&self, pass: &Pass, set: TextureSet, bounds: &[u32; TARGETS]) -> u32 {
        if pass.edge_smoothing {
            return 0x80;
        }
        let m = &pass.material;
        let vertex = pass.vertices.iter().map(|v| u32::from(v.a)).max().unwrap_or(0);
        if m.source == SourceKind::Texture {
            return match self.textures.get(&(set as i32, m.texture)) {
                None => vertex,
                Some(t) => vertex * u32::from(t.largest_alpha) / 128,
            };
        }
        if m.source == SourceKind::Target {
            return vertex * (if m.colour_only { 0x7f } else { bounds[m.source_target as usize] }) / 128;
        }
        vertex
    }

    // The largest alpha byte a pass writes (its As): 0x80 with edge smoothing, else the vertex alpha times, when
    // textured, the texture's largest alpha / 128 or the source target's bound / 128 (0x7f when colour only).
    // Each target's bound is the largest alpha drawn into it since configure().
    pub fn written_alpha(&self, pass: &Pass, set: TextureSet) -> u32 {
        self.written_alpha_with(pass, set, &self.alpha_bound)
    }

    // The largest blend factor of a pass, in bytes: 0 when opaque, the constant for the fixed modes, the target's
    // bound for AddDestinationAlpha, else As.
    pub fn blend_alpha(&self, pass: &Pass, set: TextureSet) -> u32 {
        match pass.material.blend {
            BlendOp::Opaque => 0,
            BlendOp::FixedOver | BlendOp::FixedAdd | BlendOp::SubtractFixed => u32::from(pass.material.blend_constant),
            BlendOp::AddDestinationAlpha => self.alpha_bound[pass.target as usize],
            _ => self.written_alpha(pass, set),
        }
    }

    // Targets store alpha / 128 in UNORM, so a byte above 0x80 would saturate. The frame's passes are walked first:
    // each target's bound grows with what is written to it, and in Debug a frame that would write or multiply by more
    // than 0x80 is refused before anything is recorded.
    fn check_alpha(&mut self, frame: &Frame) -> Result<()> {
        let mut bounds = self.alpha_bound;
        for pass in &frame.passes {
            if pass.vertices.is_empty() {
                continue;
            }
            if pass.material.per_pixel_alpha && pass.material.blend != BlendOp::Opaque {
                return Err(Error::Logic(format!(
                    "pass {}: per-pixel alpha with blending is not drawn (every opening pass that sets it has blending off)",
                    pass.name
                )));
            }
            let written = self.written_alpha_with(pass, frame.texture_set, &bounds);
            let blend = pass.material.blend;
            let mut factor = written;
            if blend == BlendOp::Opaque {
                factor = 0;
            } else if blend == BlendOp::FixedOver || blend == BlendOp::FixedAdd || blend == BlendOp::SubtractFixed {
                factor = u32::from(pass.material.blend_constant);
            } else if blend == BlendOp::AddDestinationAlpha {
                factor = bounds[pass.target as usize];
            }
            if cfg!(debug_assertions) && (written > 0x80 || factor > 0x80) {
                return Err(Error::Logic(format!("pass {} writes alpha 0x{written:x} and blends by 0x{factor:x}, above 0x80", pass.name)));
            }
            let bound = &mut bounds[pass.target as usize];
            *bound = (*bound).max(written.min(0x80));
        }
        self.alpha_bound = bounds;
        Ok(())
    }

    pub fn record(&mut self, cmd: vk::CommandBuffer, frame: &Frame) -> Result<()> {
        if self.output.width == 0 {
            return Err(runtime("the renderer has no output"));
        }
        self.check_alpha(frame)?;
        self.ensure_targets(cmd, frame)?;
        let mut vertices = std::mem::take(&mut self.staging);
        let mut ranges = std::mem::take(&mut self.ranges);
        vertices.clear();
        ranges.clear();
        for pass in &frame.passes {
            let first = vertices.len();
            append_vertices(&mut vertices, pass, frame.depth_bits);
            ranges.push((first as u32, (vertices.len() - first) as u32));
        }
        let result = self.record_passes(cmd, frame, &vertices, &ranges);
        self.staging = vertices;
        self.ranges = ranges;
        result
    }

    fn record_passes(&mut self, cmd: vk::CommandBuffer, frame: &Frame, vertices: &[GpuVertex], ranges: &[(u32, u32)]) -> Result<()> {
        if vertices.is_empty() {
            return Ok(());
        }
        let slot = self.slot;
        self.slot = (self.slot + 1) % self.vertices.len();
        let bytes = std::mem::size_of_val(vertices) as vk::DeviceSize;
        if self.vertices[slot].size < bytes {
            let mut old = std::mem::take(&mut self.vertices[slot]);
            self.destroy_buffer(&mut old);
            self.vertices[slot] = self.create_buffer((bytes * 2).max(1 << 20), vk::BufferUsageFlags::VERTEX_BUFFER)?;
        }
        let buffer = &self.vertices[slot];
        // SAFETY: the buffer is mapped and at least `bytes` long; GpuVertex is plain data.
        unsafe { std::ptr::copy_nonoverlapping(vertices.as_ptr() as *const u8, buffer.mapped, bytes as usize) };
        check(self.device.allocator().flush_allocation(buffer.allocation.as_ref().expect("an allocation"), 0, vk::WHOLE_SIZE), "flush")?;
        let d = self.device.device();
        // SAFETY: `cmd` is recording.
        unsafe { d.cmd_bind_vertex_buffers(cmd, 0, &[buffer.buffer], &[0]) };

        let mut open: Option<TargetName> = None;
        let mut bound = vk::Pipeline::null();
        let mut pushed_image: Option<(vk::Sampler, vk::ImageView)> = None;
        let mut last_constant = 0.0f32;
        let mut constant_valid = false;
        let mut last_push = PushState::default();
        let mut push_valid = false;
        for (i, pass) in frame.passes.iter().enumerate() {
            let m = &pass.material;
            if ranges[i].1 == 0 {
                continue;
            }
            let drawn = pass.target;
            let source = (m.source == SourceKind::Target).then_some(m.source_target);
            if source == Some(drawn) {
                return Err(Error::Logic(format!("pass {} reads the target it draws to", pass.name)));
            }
            let settle = source.is_some_and(|s| self.targets[s as usize].resolved.layout != vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
            if open.is_some() && (open != Some(drawn) || settle) {
                // SAFETY: `cmd` is inside a rendering scope.
                unsafe { d.cmd_end_rendering(cmd) };
                open = None;
            }
            if settle {
                let s = source.expect("a source") as usize;
                let mut image = std::mem::take(&mut self.targets[s].resolved);
                self.transition(
                    cmd,
                    &mut image,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                );
                self.targets[s].resolved = image;
            }
            if open.is_none() {
                self.begin_rendering(cmd, drawn);
                open = Some(drawn);
                bound = vk::Pipeline::null();
            }
            if self.device.labels() {
                self.device.begin_label(cmd, &pass.name);
            }
            let mut push =
                PushState { scene: [frame.width as f32, frame.height as f32, self.output.width as f32, self.output.height as f32], ..Default::default() };
            let mut view = self.blank.view;
            if m.source == SourceKind::Texture {
                let layout = m.texture >= GLYPH_LAYOUT_TEXTURE && m.texture < GLYPH_LAYOUT_TEXTURE + frame.glyph_layouts.len() as i32;
                let glyphs = (m.texture == GLYPH_TEXTURE || layout) && frame.texture_set != TextureSet::Opening;
                let set = if glyphs { CLOCK_SET } else { frame.texture_set as i32 };
                let Some(found) = self.textures.get(&(set, m.texture)) else {
                    return Err(runtime(format!("pass {}: no texture {}", pass.name, m.texture)));
                };
                if glyphs && (frame.glyphs != self.glyphs || frame.glyph_layouts != self.glyph_layouts) {
                    return Err(runtime(format!("pass {}: the glyph cache set is not the frame's", pass.name)));
                }
                let image = &found.image;
                view = image.view;
                push.source[0] = found.logical[0];
                push.source[2] = image.width as f32;
                push.source[1] = found.logical[1];
                push.source[3] = image.height as f32;
                push.mode[0] = 1;
                push.shade[2] = 255;
            } else if let Some(s) = source {
                view = self.targets[s as usize].resolved.view;
                push.source = [frame.width as f32, frame.height as f32, self.output.width as f32, self.output.height as f32];
                push.mode[0] = 2;
                push.shade[2] = 128;
                if frame.texture_set == TextureSet::Opening {
                    push.extent[0] = TARGET_TEXTURE_WIDTH;
                    push.extent[1] = TARGET_TEXTURE_HEIGHT;
                }
            }
            for k in 0..4 {
                push.region[k] = m.region[k] as f32;
            }
            push.mode[1] = i32::from(m.coordinates == CoordinateKind::Projective);
            push.mode[2] = i32::from(m.sampling == Sampling::ClampToRegion);
            push.mode[3] = i32::from(m.colour_only);
            push.shade[0] = i32::from(pass.edge_smoothing);

            let sampler = self.samplers[usize::from(m.bilinear) | if m.sampling == Sampling::Repeat { 0 } else { 2 }];
            if pushed_image != Some((sampler, view)) {
                let image = [vk::DescriptorImageInfo { sampler, image_view: view, image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL }];
                let write = [vk::WriteDescriptorSet::default().dst_binding(0).descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER).image_info(&image)];
                // SAFETY: `cmd` is recording; the layout's set 0 is a push descriptor set.
                unsafe { self.device.cmd_push_descriptor_set(cmd, self.layout, 0, &write) };
                pushed_image = Some((sampler, view));
            }

            let constant = f32::from(m.blend_constant) / 128.0;
            if !constant_valid || constant.to_bits() != last_constant.to_bits() {
                // SAFETY: `cmd` is recording.
                unsafe { d.cmd_set_blend_constants(cmd, &[constant, constant, constant, constant]) };
                last_constant = constant;
                constant_valid = true;
            }
            let p = self.pipeline(pass, m.blend)?;
            if p != bound {
                // SAFETY: `cmd` is recording.
                unsafe { d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, p) };
                bound = p;
            }
            push.shade[1] = i32::from(premultiplied(m.blend));
            if !push_valid || push.bytes() != last_push.bytes() {
                // SAFETY: `cmd` is recording; the range is the layout's.
                unsafe { d.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT, 0, &push.bytes()) };
                last_push = push;
                push_valid = true;
            }
            // SAFETY: `cmd` is recording inside a rendering scope with a bound pipeline.
            unsafe { d.cmd_draw(cmd, ranges[i].1, 1, ranges[i].0, 0) };
            self.device.end_label(cmd);
        }
        if open.is_some() {
            // SAFETY: `cmd` is inside a rendering scope.
            unsafe { d.cmd_end_rendering(cmd) };
        }
        Ok(())
    }

    pub fn draw(&mut self, frame: &Frame) -> Result<()> {
        let cmd = self.begin_submit()?;
        self.record(cmd, frame)?;
        self.end_submit(cmd)
    }

    pub fn read_target(&mut self, name: TargetName) -> Result<Vec<u8>> {
        if self.output.width == 0 {
            return Err(runtime("the renderer has no output"));
        }
        let bytes = self.output.width as usize * self.output.height as usize * 4;
        if !self.targets[name as usize].resolved.exists() {
            return Ok(vec![0u8; bytes]);
        }
        let mut readback = self.create_buffer(bytes as vk::DeviceSize, vk::BufferUsageFlags::TRANSFER_DST)?;
        let cmd = match self.begin_submit() {
            Ok(c) => c,
            Err(e) => {
                self.destroy_buffer(&mut readback);
                return Err(e);
            }
        };
        let mut image = std::mem::take(&mut self.targets[name as usize].resolved);
        self.transition(cmd, &mut image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_READ);
        let copy = vk::BufferImageCopy {
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
            image_extent: vk::Extent3D { width: self.output.width, height: self.output.height, depth: 1 },
            ..Default::default()
        };
        // SAFETY: `cmd` is recording; the image is in TRANSFER_SRC_OPTIMAL.
        unsafe { self.device.device().cmd_copy_image_to_buffer(cmd, image.image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, readback.buffer, &[copy]) };
        self.targets[name as usize].resolved = image;
        let submitted = self.end_submit(cmd).and_then(|_| {
            check(self.device.allocator().invalidate_allocation(readback.allocation.as_ref().expect("an allocation"), 0, vk::WHOLE_SIZE), "invalidate")
        });
        let mut rgba = vec![0u8; bytes];
        if submitted.is_ok() {
            // SAFETY: the buffer is mapped, `bytes` long, and its copy completed.
            unsafe { std::ptr::copy_nonoverlapping(readback.mapped, rgba.as_mut_ptr(), bytes) };
        }
        self.destroy_buffer(&mut readback);
        submitted?;
        for a in rgba.iter_mut().skip(3).step_by(4) {
            *a = (f64::from(*a) * 128.0 / 255.0).round() as u8;
        }
        Ok(rgba)
    }

    // `image` is in COLOR_ATTACHMENT_OPTIMAL and is left so; the target is fitted in `extent` at `aspect`
    // (width / height of the shown picture) with black around it.
    pub fn present(&mut self, cmd: vk::CommandBuffer, image: vk::Image, extent: vk::Extent2D, shown: TargetName, aspect: f32) -> Result<()> {
        if !self.targets[shown as usize].resolved.exists() {
            return Err(runtime("the shown target has not been drawn"));
        }
        let mut source = std::mem::take(&mut self.targets[shown as usize].resolved);
        self.transition(cmd, &mut source, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, vk::PipelineStageFlags2::BLIT, vk::AccessFlags2::TRANSFER_READ);
        let d = self.device.device();
        let colour = vk::ImageAspectFlags::COLOR;
        barrier(
            d,
            cmd,
            image,
            colour,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::CLEAR,
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let black = vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] };
        // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL.
        unsafe { d.cmd_clear_color_image(cmd, image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &black, &[colour_range()]) };
        barrier(
            d,
            cmd,
            image,
            colour,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::CLEAR,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::BLIT,
            vk::AccessFlags2::TRANSFER_WRITE,
        );

        let mut w = extent.width as f32;
        let mut h = w / aspect;
        if h > extent.height as f32 {
            h = extent.height as f32;
            w = h * aspect;
        }
        let x0 = ((extent.width as f32 - w) / 2.0) as i32;
        let y0 = ((extent.height as f32 - h) / 2.0) as i32;
        let layers = vk::ImageSubresourceLayers { aspect_mask: colour, mip_level: 0, base_array_layer: 0, layer_count: 1 };
        let blit = vk::ImageBlit {
            src_subresource: layers,
            src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, vk::Offset3D { x: source.width as i32, y: source.height as i32, z: 1 }],
            dst_subresource: layers,
            dst_offsets: [vk::Offset3D { x: x0, y: y0, z: 0 }, vk::Offset3D { x: (x0 + 1).max(x0 + w as i32), y: (y0 + 1).max(y0 + h as i32), z: 1 }],
        };
        // SAFETY: `cmd` is recording; the layouts are those the barriers set.
        unsafe {
            d.cmd_blit_image(
                cmd,
                source.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            )
        };
        barrier(
            d,
            cmd,
            image,
            colour,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::BLIT,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        self.targets[shown as usize].resolved = source;
        Ok(())
    }
}

impl Drop for NativeRenderer<'_> {
    fn drop(&mut self) {
        let d = self.device.device();
        // SAFETY: tearing down in the order ~NativeRenderer does, after the device is idle.
        unsafe {
            d.device_wait_idle().ok();
            self.destroy_targets();
            let mut buffers = std::mem::take(&mut self.vertices);
            for b in &mut buffers {
                self.destroy_buffer(b);
            }
            let mut textures = std::mem::take(&mut self.textures);
            for t in textures.values_mut() {
                self.destroy_image(&mut t.image);
            }
            let mut blank = std::mem::take(&mut self.blank);
            self.destroy_image(&mut blank);
            for &p in self.pipelines.values() {
                d.destroy_pipeline(p, None);
            }
            for &s in &self.samplers {
                d.destroy_sampler(s, None);
            }
            d.destroy_command_pool(self.pool, None);
            d.destroy_pipeline_layout(self.layout, None);
            d.destroy_descriptor_set_layout(self.set_layout, None);
            d.destroy_shader_module(self.vertex, None);
            d.destroy_shader_module(self.fragment, None);
        }
    }
}

fn colour_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange { aspect_mask: vk::ImageAspectFlags::COLOR, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 }
}

// std::sscanf(name, "tex%d-%dx%d.png", &number, &width, &height) == 3.
fn scan_texture_name(name: &str) -> Option<(i32, i32, i32)> {
    fn int(s: &str) -> Option<(i32, &str)> {
        let t = s.trim_start();
        let digits_at = usize::from(t.starts_with(['+', '-']));
        let end = digits_at + t[digits_at..].bytes().take_while(u8::is_ascii_digit).count();
        if end == digits_at {
            return None;
        }
        Some((t[..end].parse::<i64>().ok()? as i32, &t[end..]))
    }
    let rest = name.strip_prefix("tex")?;
    let (number, rest) = int(rest)?;
    let rest = rest.strip_prefix('-')?;
    let (width, rest) = int(rest)?;
    let rest = rest.strip_prefix('x')?;
    let (height, _) = int(rest)?;
    Some((number, width, height))
}
