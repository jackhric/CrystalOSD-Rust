// Traces the first block of the console's font (Latin, Greek, Cyrillic, the marks) into the outlines `--text sharp` draws.
// usage: crystalosd-sharp-font [--font FNTOSD] [--out file.outlines] [--svg dir]
use std::path::PathBuf;
use std::time::Instant;

use glyph_enhance::outlines::{block_zero_codes, build, cache_path, font_hash};
use scene::font::Font;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let font_path = value("--font").unwrap_or_else(|| PathBuf::from("Resources/FNTOSD"));
    let file = match std::fs::read(&font_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {e}", font_path.display());
            std::process::exit(1);
        }
    };
    let font = Font::new(&file).unwrap_or_else(|e| {
        eprintln!("{}: {}", font_path.display(), e.0);
        std::process::exit(1);
    });
    let hash = font_hash(&file);
    let out = value("--out").unwrap_or_else(|| cache_path(&assets::resources::user_data_directory(), hash));
    let codes = block_zero_codes(&font);
    let start = Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let outlines = build(&font, hash, &codes, threads);
    println!("traced {} of {} glyphs of block 0 in {:.2} s on {threads} threads", outlines.glyphs.len(), codes.len(), start.elapsed().as_secs_f64());
    let kept: Vec<String> = codes.iter().filter(|c| !outlines.glyphs.contains_key(c)).map(|c| format!("{c:04x}")).collect();
    println!("pictures kept (no outline): {}", if kept.is_empty() { "none".to_owned() } else { kept.join(" ") });
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, outlines.to_text()).unwrap_or_else(|e| {
        eprintln!("{}: {e}", out.display());
        std::process::exit(1);
    });
    println!("wrote {}", out.display());
    if let Some(dir) = value("--svg") {
        let _ = std::fs::create_dir_all(&dir);
        for (code, g) in &outlines.glyphs {
            let mut path = String::new();
            for c in g.core.iter().chain(&g.outer) {
                for (i, p) in c.iter().enumerate() {
                    path += &format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, p[0], p[1]);
                }
                path += "Z ";
            }
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\"><path d=\"{path}\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\"/></svg>\n",
                g.width,
                g.height,
                g.width * 10,
                g.height * 10
            );
            let _ = std::fs::write(dir.join(format!("{code:04x}.svg")), svg);
        }
        println!("wrote {} SVG files to {}", outlines.glyphs.len(), dir.display());
    }
}
