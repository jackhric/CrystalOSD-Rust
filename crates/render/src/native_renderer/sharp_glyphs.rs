// `--text sharp`: the glyph cache's textures (the frame's cache and the layouts it passed through after it) replaced by
// larger ones drawn from outlines (glyph_enhance: TeX Gyre Heros fitted to the console's glyphs, else the traced ones).
// Each texture keeps its logical size, so the scene's texture coordinates, the region clamp (to the cell, in logical
// texels) and everything else about the passes are unchanged; only the texels are finer.
use std::sync::Arc;

use ash::vk;
use glyph_enhance::atlas::{Pictures, Rect, SharpAtlas};
use glyph_enhance::picture::SharpFont;
use scene::font::Font;
use scene::frame::{GLYPH_LAYOUT_TEXTURE, GLYPH_TEXTURE, GlyphCache};

use super::{CLOCK_SET, NativeRenderer, Texture};
use crate::error::{Result, check, runtime};

const LARGEST_SCALE: u32 = 8;

// Drawn when the atlas is made: ASCII and the marks (0xD803..0xD818), what the English menus use.
const PREPARED: [i32; 118] = {
    let mut codes = [0; 118];
    let mut i = 0;
    while i < 96 {
        codes[i] = 0x20 + i as i32;
        i += 1;
    }
    while i < 118 {
        codes[i] = 0xd803 + (i - 96) as i32;
        i += 1;
    }
    codes
};

pub(super) struct SharpGlyphs {
    shapes: Arc<SharpFont>,
    pictures: Option<Pictures>,
    atlas: Option<SharpAtlas>,
    layouts: Vec<SharpAtlas>,
}

impl SharpGlyphs {
    // The pictures at `scale`, drawn again (ASCII and the marks at once) when the scale changed, with every atlas.
    fn at_scale(&mut self, font: &Font, scale: u32) {
        if self.pictures.as_ref().is_none_or(|p| p.scale != scale) {
            let mut pictures = Pictures::new(scale);
            let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
            pictures.prepare(font, &self.shapes, &PREPARED, threads);
            self.pictures = Some(pictures);
            self.atlas = None;
            self.layouts.clear();
        }
    }
}

// Texels per logical texel for an output: the frame's scale (height / 448) rounded up, so a glyph at size 1 is never
// magnified; none below 2 (at the console's own size the cache's pictures are the text).
pub fn sharp_scale(output_height: u32) -> u32 {
    output_height.div_ceil(448).min(LARGEST_SCALE)
}

impl NativeRenderer<'_> {
    // Sharp text from `shapes` (None: the console's glyph pictures, as without the option).
    pub fn set_sharp_text(&mut self, shapes: Option<Arc<SharpFont>>) {
        self.sharp = shapes.map(|shapes| SharpGlyphs { shapes, pictures: None, atlas: None, layouts: Vec::new() });
        self.glyphs = GlyphCache::default();
        self.glyph_layouts.clear();
    }

    pub fn sharp_text_scale(&self) -> Option<u32> {
        self.sharp.as_ref().map(|_| sharp_scale(self.output.height)).filter(|&k| k >= 2)
    }

    // Returns false when the console's texture is to be made instead.
    pub(super) fn set_sharp_glyph_cache(&mut self, font: &Font, cache: &GlyphCache) -> Result<bool> {
        let Some(scale) = self.sharp_text_scale() else { return Ok(false) };
        let mut sharp = self.sharp.take().expect("sharp text");
        sharp.at_scale(font, scale);
        let SharpGlyphs { shapes, pictures, atlas, .. } = &mut sharp;
        let atlas = atlas.get_or_insert_with(|| SharpAtlas::new(scale));
        let changed = atlas.update(pictures.as_mut().expect("pictures"), font, shapes, cache);
        let result = self.upload_sharp(GLYPH_TEXTURE, atlas, &changed, font, cache);
        self.sharp = Some(sharp);
        result?;
        self.glyphs = cache.clone();
        Ok(true)
    }

    // The frame's later layouts (Frame::glyph_layouts), each its own sharp texture, so that a string drawn after the cache
    // was laid out again is as sharp as the rest. Returns false when the console's textures are to be made instead.
    pub(super) fn set_sharp_glyph_layouts(&mut self, font: &Font, layouts: &[GlyphCache]) -> Result<bool> {
        let Some(scale) = self.sharp_text_scale() else { return Ok(false) };
        let mut sharp = self.sharp.take().expect("sharp text");
        sharp.at_scale(font, scale);
        let mut result = Ok(());
        for (k, cache) in layouts.iter().enumerate() {
            let SharpGlyphs { shapes, pictures, layouts: atlases, .. } = &mut sharp;
            if atlases.len() <= k {
                atlases.push(SharpAtlas::new(scale));
            }
            let changed = atlases[k].update(pictures.as_mut().expect("pictures"), font, shapes, cache);
            result = self.upload_sharp(GLYPH_LAYOUT_TEXTURE + k as i32, &sharp.layouts[k], &changed, font, cache);
            if result.is_err() {
                break;
            }
        }
        self.sharp = Some(sharp);
        result?;
        self.glyph_layouts = layouts.to_vec();
        Ok(true)
    }

    fn upload_sharp(&mut self, number: i32, atlas: &SharpAtlas, changed: &[Rect], font: &Font, cache: &GlyphCache) -> Result<()> {
        let key = (CLOCK_SET, number);
        let logical = [(1u32 << cache.log_width) as f32, (1u32 << cache.log_height) as f32];
        // The largest alpha the alpha bound reads is the console texture's, as without the option.
        let console = scene::text::glyph_cache_image(font, cache).map_err(|e| crate::error::Error::Runtime(e.0))?;
        let largest_alpha = console.iter().skip(3).step_by(4).copied().max().unwrap_or(0);
        let fits = self.textures.get(&key).is_some_and(|t| t.image.width == atlas.width && t.image.height == atlas.height && t.logical == logical);
        if !fits {
            self.set_texture(number, atlas.width, atlas.height, &atlas.rgba)?;
            let t = self.textures.get_mut(&key).ok_or_else(|| runtime("the sharp glyph texture"))?;
            t.logical = logical;
            t.largest_alpha = largest_alpha;
            return Ok(());
        }
        if changed.is_empty() {
            if let Some(t) = self.textures.get_mut(&key) {
                t.largest_alpha = largest_alpha;
            }
            return Ok(());
        }
        let bytes: usize = changed.iter().map(|r| r.width as usize * r.height as usize * 4).sum();
        let mut staging = self.create_buffer(bytes as vk::DeviceSize, vk::BufferUsageFlags::TRANSFER_SRC)?;
        let mut copies = Vec::with_capacity(changed.len());
        let mut at = 0usize;
        for r in changed {
            for y in 0..r.height as usize {
                let from = ((r.y as usize + y) * atlas.width as usize + r.x as usize) * 4;
                let row = r.width as usize * 4;
                // SAFETY: the buffer is mapped and `bytes` long; each row is written once inside it.
                unsafe { std::ptr::copy_nonoverlapping(atlas.rgba[from..from + row].as_ptr(), staging.mapped.add(at + y * row), row) };
            }
            copies.push(vk::BufferImageCopy {
                buffer_offset: at as vk::DeviceSize,
                image_subresource: vk::ImageSubresourceLayers { aspect_mask: vk::ImageAspectFlags::COLOR, mip_level: 0, base_array_layer: 0, layer_count: 1 },
                image_offset: vk::Offset3D { x: r.x as i32, y: r.y as i32, z: 0 },
                image_extent: vk::Extent3D { width: r.width, height: r.height, depth: 1 },
                ..Default::default()
            });
            at += r.width as usize * r.height as usize * 4;
        }
        check(self.device.allocator().flush_allocation(staging.allocation.as_ref().expect("an allocation"), 0, vk::WHOLE_SIZE), "flush")?;
        // SAFETY: a valid device; the texture may be in use by a frame in flight.
        unsafe { self.device.device().device_wait_idle() }.ok();
        let mut texture: Texture = self.textures.remove(&key).ok_or_else(|| runtime("the sharp glyph texture"))?;
        let cmd = self.begin_submit()?;
        self.transition(cmd, &mut texture.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::PipelineStageFlags2::TRANSFER, vk::AccessFlags2::TRANSFER_WRITE);
        // SAFETY: `cmd` is recording; the image is in TRANSFER_DST_OPTIMAL and every region is inside it.
        unsafe { self.device.device().cmd_copy_buffer_to_image(cmd, staging.buffer, texture.image.image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &copies) };
        self.transition(
            cmd,
            &mut texture.image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
        let submitted = self.end_submit(cmd);
        self.destroy_buffer(&mut staging);
        texture.largest_alpha = largest_alpha;
        self.textures.insert(key, texture);
        submitted
    }
}
