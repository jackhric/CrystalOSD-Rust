use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use glyph_enhance::outlines::{Loaded, cache_path, font_hash, load_or_build};
use render::NativeRenderer;
use scene::font::Font;

// `--text sharp`: the outlines of this FNTOSD from `outlines` (or the user data directory), traced now and kept there when
// they are not yet; the renderer draws the glyph cache from them at outputs of twice the console's size and more.
pub fn enable(renderer: &mut NativeRenderer<'_>, font: &Font, font_file: &[u8], outlines: &Path) {
    let path =
        if outlines.as_os_str().is_empty() { cache_path(&assets::resources::user_data_directory(), font_hash(font_file)) } else { outlines.to_path_buf() };
    let start = Instant::now();
    let loaded = load_or_build(font, font_file, &path);
    let seconds = start.elapsed().as_secs_f64();
    let outlines = match loaded {
        Loaded::Read(o) => {
            println!("text: sharp, {} glyph outlines read from {} in {seconds:.2} s", o.glyphs.len(), path.display());
            o
        }
        Loaded::Built(o, written) => {
            match written {
                Ok(()) => println!("text: sharp, {} glyph outlines traced in {seconds:.2} s and kept in {}", o.glyphs.len(), path.display()),
                Err(e) => println!("text: sharp, {} glyph outlines traced in {seconds:.2} s (not kept: {e})", o.glyphs.len()),
            }
            o
        }
    };
    renderer.set_sharp_text(Some(Arc::new(outlines)));
}
