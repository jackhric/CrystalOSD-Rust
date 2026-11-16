use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use glyph_enhance::heros_table::{self, Choice, Found, heros_for, load_or_fit};
use glyph_enhance::outlines::{Loaded, cache_path, font_hash, load_or_build};
use render::NativeRenderer;
use scene::font::Font;

// `--text sharp`: TeX Gyre Heros where it was fitted to this FNTOSD's glyphs (the bundled fit, or one made now and kept in
// the user data directory), inside the dark outlines traced from `outlines` (or the user data directory), traced now and
// kept there when they are not yet; the traced outlines for the glyphs Heros does not draw like the console. The renderer
// draws the glyph cache from them at outputs of twice the console's size and more.
pub fn enable(renderer: &mut NativeRenderer<'_>, font: &Font, font_file: &[u8], outlines: &Path) {
    let data = assets::resources::user_data_directory();
    let hash = font_hash(font_file);
    let path = if outlines.as_os_str().is_empty() { cache_path(&data, hash) } else { outlines.to_path_buf() };
    let start = Instant::now();
    let loaded = load_or_build(font, font_file, &path);
    let seconds = start.elapsed().as_secs_f64();
    let traced = match loaded {
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
    let start = Instant::now();
    let heros = heros_for(font);
    let fit_path = heros_table::cache_path(&data, hash);
    let table = match load_or_fit(font, font_file, &traced, &heros, &fit_path) {
        Found::Bundled(t) => t,
        Found::Read(t) => {
            println!("text: TeX Gyre Heros fit read from {}", fit_path.display());
            t
        }
        Found::Fitted(t, written) => {
            match written {
                Ok(()) => println!("text: TeX Gyre Heros fitted to this FNTOSD and kept in {}", fit_path.display()),
                Err(e) => println!("text: TeX Gyre Heros fitted to this FNTOSD (not kept: {e})"),
            }
            t
        }
    };
    let shapes = table.sharp_font(font, &heros, &traced);
    println!(
        "text: sharp, {} glyphs from TeX Gyre Heros, {} traced, {} pictures ({:.2} s)",
        table.count(Choice::Heros),
        table.count(Choice::Traced),
        table.count(Choice::Picture),
        start.elapsed().as_secs_f64()
    );
    renderer.set_sharp_text(Some(Arc::new(shapes)));
}
