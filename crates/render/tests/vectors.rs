// The C++ NativeRenderer's output against this one, call for call, on the same GPU.
//
// The vectors (Sony-derived frames and textures: never committed) are streams of every NativeRenderer call a run makes,
// written by the C++ probe kept outside the repository (lane-render/probes: Stream.hpp, Wrap.cpp, Replay.cpp,
// Synthetic.cpp, build.sh). To regenerate them, build the C++ Release tree, then `sh probes/build.sh` and, from an empty
// working directory:
//   CRYSTALOSD_STREAM=<dir>/golden-native.stream probes/bin/GoldenRecorder --no-validation --resources <Resources>
//       --shaders <bin/shaders> --golden record golden-native.txt --golden-output native     (and x2-msaa4)
//   probes/bin/Synthetic <dir>/synthetic.stream
//   probes/bin/Replay <bin/shaders> <Resources> <dir>/<name>.stream <name>.txt
// GoldenRecorder is the real CrystalOSD app relinked with -Wl,--wrap on NativeRenderer's members: the golden gate's
// 1719 frames (opening, menus, System Configuration and its cubes) with their textures and glyph caches, and the hash of
// every Display read back (the golden file's pixels=). Synthetic covers every BlendOp, DepthTest, Sampling, SourceKind,
// CoordinateKind, topology, target and flag at 640x448x1, 1280x896x4 and 960x672x2, and present() into an offscreen image.
// Replay renders a stream through the C++ renderer on a headless Device; its hashes are tests/vectors/<name>.txt.
//
// Run with CRYSTALOSD_RENDER_VECTORS=<dir> and CRYSTALOSD_RESOURCES=<Resources> (the font for the glyph cache); the
// tests skip without them. The hashes are this machine's (the header of each file names the GPU and driver): another
// GPU or driver gives other pixels, as the C++ golden baselines are per machine.

mod support;

use std::path::{Path, PathBuf};

use render::{Device, NativeRenderer, vk};
use scene::font::Font;
use support::{Record, Stream, fnv};
use vk_mem::Alloc;

fn present_hash(device: &Device, renderer: &mut NativeRenderer<'_>, w: u32, h: u32, shown: scene::frame::TargetName, aspect: f32) -> u64 {
    let d = device.device();
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::B8G8R8A8_UNORM)
        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC);
    let image_alloc = vk_mem::AllocationCreateInfo { usage: vk_mem::MemoryUsage::AutoPreferDevice, ..Default::default() };
    let buffer_info = vk::BufferCreateInfo::default().size(u64::from(w) * u64::from(h) * 4).usage(vk::BufferUsageFlags::TRANSFER_DST);
    let buffer_alloc = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::Auto,
        flags: vk_mem::AllocationCreateFlags::MAPPED | vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
        ..Default::default()
    };
    let barrier = |cmd, image, from, to, ss, sa, ds, da| {
        let b = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(ss)
            .src_access_mask(sa)
            .dst_stage_mask(ds)
            .dst_access_mask(da)
            .old_layout(from)
            .new_layout(to)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1))];
        // SAFETY: `cmd` is recording.
        unsafe { d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&b)) };
    };
    // SAFETY: test-only objects on a valid device, destroyed before returning.
    unsafe {
        let (image, mut ialloc) = device.allocator().create_image(&info, &image_alloc).unwrap();
        let (buffer, mut balloc) = device.allocator().create_buffer(&buffer_info, &buffer_alloc).unwrap();
        let pool = d.create_command_pool(&vk::CommandPoolCreateInfo::default().queue_family_index(device.queue_family()), None).unwrap();
        let cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default().command_pool(pool).level(vk::CommandBufferLevel::PRIMARY).command_buffer_count(1),
            )
            .unwrap()[0];
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)).unwrap();
        barrier(
            cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        renderer.present(cmd, image, vk::Extent2D { width: w, height: h }, shown, aspect).unwrap();
        barrier(
            cmd,
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::TRANSFER,
            vk::AccessFlags2::TRANSFER_READ,
        );
        let copy = vk::BufferImageCopy {
            image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
            image_extent: vk::Extent3D { width: w, height: h, depth: 1 },
            ..Default::default()
        };
        d.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, buffer, &[copy]);
        d.end_command_buffer(cmd).unwrap();
        let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
        d.queue_submit2(device.queue(), &[vk::SubmitInfo2::default().command_buffer_infos(&cmds)], vk::Fence::null()).unwrap();
        d.queue_wait_idle(device.queue()).unwrap();
        device.allocator().invalidate_allocation(&balloc, 0, vk::WHOLE_SIZE).unwrap();
        let mapped = device.allocator().get_allocation_info(&balloc).mapped_data as *const u8;
        let hash = fnv(std::slice::from_raw_parts(mapped, w as usize * h as usize * 4));
        d.destroy_command_pool(pool, None);
        device.allocator().destroy_buffer(buffer, &mut balloc);
        device.allocator().destroy_image(image, &mut ialloc);
        hash
    }
}

struct Expected {
    lines: Vec<(String, u64)>,
}

fn expected(name: &str) -> Expected {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors").join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let lines = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (format!("{} {}", f[1], f[2]), u64::from_str_radix(f[3], 16).unwrap())
        })
        .collect();
    Expected { lines }
}

fn vectors() -> Option<PathBuf> {
    match std::env::var_os("CRYSTALOSD_RENDER_VECTORS") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("skipped: CRYSTALOSD_RENDER_VECTORS is not set");
            None
        }
    }
}

fn font() -> Option<Font> {
    let folder = std::env::var_os("CRYSTALOSD_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../CrystalOSD/Resources"));
    let set = assets::resources::decode_folder(&folder).ok()?;
    Font::new(&set.find("FNTOSD")?.data).ok()
}

fn replay(name: &str) {
    let Some(dir) = vectors() else { return };
    let path = dir.join(format!("{name}.stream"));
    if !path.exists() {
        eprintln!("skipped: no {}", path.display());
        return;
    }
    let Some(device) = support::device() else { return };
    let want = expected(name);
    let font = font();
    let mut renderer = NativeRenderer::new(&device, support::shaders()).unwrap();
    let (mut index, mut draws, mut differ, mut glyph_images) = (0usize, 0usize, Vec::new(), 0usize);
    let mut recorded_differ = 0usize;
    let mut label = String::new();
    for record in Stream::open(&path) {
        match record {
            Record::Configure(w, h, s) => renderer.configure(&render::NativeOutput { width: w, height: h, samples: s }).unwrap(),
            Record::SetTexture(set, n, w, h, rgba) => renderer.set_texture_in(set, n, w, h, &rgba).unwrap(),
            Record::GlyphCache(cache, image) => {
                let font = font.as_ref().expect("CRYSTALOSD_RESOURCES (FNTOSD) is needed for the glyph cache");
                if !image.is_empty() {
                    glyph_images += 1;
                    assert!(scene::text::glyph_cache_image(font, &cache).unwrap() == image, "glyph cache image differs from C++");
                }
                renderer.set_glyph_cache(font, &cache).unwrap();
            }
            Record::Draw(frame) => {
                renderer.draw(&frame).unwrap_or_else(|e| panic!("draw {draws} ({label}): {e}"));
                draws += 1;
            }
            Record::Label(l) => label = l,
            Record::ReadTarget(t, recorded) | Record::Present(_, _, t, _, recorded) if index >= want.lines.len() => {
                let _ = (t, recorded);
                panic!("{name}: more reads than the expected file has");
            }
            Record::ReadTarget(t, recorded) => {
                let got = fnv(&renderer.read_target(t).unwrap());
                let (kind, hash) = &want.lines[index];
                assert_eq!(kind, &format!("read {}", t as i32), "{name}: record {index} is not the expected file's");
                if got != *hash {
                    differ.push(format!("{index} ({label}, after draw {draws}) read {t:?}: {got:016x}, C++ {hash:016x}"));
                }
                recorded_differ += usize::from(recorded != 0 && recorded != *hash);
                index += 1;
            }
            Record::Present(w, h, t, aspect, recorded) => {
                let got = present_hash(&device, &mut renderer, w, h, t, aspect);
                let (kind, hash) = &want.lines[index];
                assert_eq!(kind, &format!("present {}", t as i32), "{name}: record {index} is not the expected file's");
                if got != *hash {
                    differ.push(format!("{index} ({label}) present {w}x{h} {t:?} {aspect}: {got:016x}, C++ {hash:016x}"));
                }
                recorded_differ += usize::from(recorded != 0 && recorded != *hash);
                index += 1;
            }
        }
    }
    println!("{name}: {draws} draws, {index} hashes compared, {} differ; {glyph_images} glyph cache images equal to C++", differ.len());
    assert_eq!(index, want.lines.len(), "{name}: fewer reads than the expected file has");
    assert_eq!(recorded_differ, 0, "{name}: the expected file disagrees with the hashes recorded in the stream");
    assert_eq!(device.validation_errors(), 0);
    assert!(
        differ.is_empty(),
        "{name}: {} of {index} hashes differ from C++:\n{}",
        differ.len(),
        differ.iter().take(20).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn golden_native() {
    replay("golden-native");
}

#[test]
fn golden_x2_msaa4() {
    replay("golden-x2-msaa4");
}

#[test]
fn synthetic() {
    replay("synthetic");
}
