// Fits TeX Gyre Heros to the first block of the console's font and decides, glyph by glyph, which source `--text sharp`
// draws (crates/glyph_enhance/data/heros-fit.txt is this tool's output for Resources/FNTOSD).
// usage: crystalosd-heros-fit [--font FNTOSD] [--out heros-fit.txt] [--keep-runs]
//   --keep-runs: the bundled table's two runs, refitting the glyphs only.
use std::path::PathBuf;
use std::time::Instant;

use glyph_enhance::heros_fit::{Class, Model};
use glyph_enhance::heros_table::{BOUND, BUNDLED, Choice, FitTable, MARGIN, fit_glyphs, fit_models, heros_for, heros_hash, totals};
use glyph_enhance::outlines::{block_zero_codes, build, font_hash};
use scene::font::Font;

// Where the global fit starts: the earlier measurement of this font (28.36 x 27.89 px an em, baseline 31.6, emboldened
// 0.24 x 0.08, blur 0.13).
const START: Model = Model { scale_x: 28.36, scale_y: 27.89, baseline: 31.6, embolden_x: 0.24, embolden_y: 0.08, blur: 0.13, remap: false };

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let font_path = value("--font").unwrap_or_else(|| PathBuf::from("Resources/FNTOSD"));
    let out = value("--out").unwrap_or_else(|| PathBuf::from("crates/glyph_enhance/data/heros-fit.txt"));
    let file = std::fs::read(&font_path).unwrap_or_else(|e| {
        eprintln!("{}: {e}", font_path.display());
        std::process::exit(1);
    });
    let font = Font::new(&file).unwrap_or_else(|e| {
        eprintln!("{}: {}", font_path.display(), e.0);
        std::process::exit(1);
    });
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let hash = font_hash(&file);
    let codes = block_zero_codes(&font);
    let heros = heros_for(&font);
    let start = Instant::now();
    let traced = build(&font, hash, &codes, threads);
    println!("traced {} of {} block-0 glyphs in {:.2} s", traced.glyphs.len(), codes.len(), start.elapsed().as_secs_f64());

    let start = Instant::now();
    let models = if args.iter().any(|a| a == "--keep-runs") {
        FitTable::from_text(BUNDLED).expect("the bundled table").models
    } else {
        let mut best = None;
        for remap in [false, true] {
            let fit = fit_models(&font, &heros, &Model { remap, ..START }, threads);
            for (i, (m, (train, validate))) in fit.models.iter().zip(fit.errors).enumerate() {
                println!(
                    "run {} remap {remap}: {:.4} x {:.4} px/em, baseline {:.4}, embolden {:.4} x {:.4}, blur {:.4}; train {train:.4}, validation {validate:.4}",
                    i + 1,
                    m.scale_x,
                    m.scale_y,
                    m.baseline,
                    m.embolden_x,
                    m.embolden_y,
                    m.blur
                );
            }
            let validation = fit.errors[0].1;
            if best.as_ref().is_none_or(|(v, _)| validation < *v) {
                best = Some((validation, fit.models));
            }
        }
        let (validation, models) = best.expect("a fit");
        println!("kept remap {} (run 1 validation {validation:.4})", models[0].remap);
        models
    };
    println!("runs in {:.1} s", start.elapsed().as_secs_f64());

    let start = Instant::now();
    let glyphs = fit_glyphs(&font, &heros, &models, &traced, threads);
    let table = FitTable { fntosd: hash, heros: heros_hash(), models, glyphs };
    println!("{} glyphs fitted and decided in {:.1} s (margin {MARGIN}, bound {BOUND})", table.glyphs.len(), start.elapsed().as_secs_f64());

    let shapes = table.sharp_font(&font, &heros, &traced);
    let row = |name: &str, codes: Vec<i32>| {
        if codes.is_empty() {
            return;
        }
        let chosen = |c: Choice| codes.iter().filter(|k| table.glyphs[k].choice == c).count();
        let [sharp, traced_total, bilinear] = totals(&font, &shapes, &traced, &codes);
        let heros_only: Vec<i32> = codes.iter().copied().filter(|k| table.glyphs[k].choice == Choice::Heros).collect();
        let [h, t, b] = totals(&font, &shapes, &traced, &heros_only);
        println!(
            "{name:28} {:3} glyphs: heros {:3} traced {:3} picture {:3} | as drawn {:.4}, all traced {:.4}, bilinear {:.4} | on the Heros ones: heros {:.4} traced {:.4} bilinear {:.4}",
            codes.len(),
            chosen(Choice::Heros),
            chosen(Choice::Traced),
            chosen(Choice::Picture),
            sharp.mean(),
            traced_total.mean(),
            bilinear.mean(),
            h.mean(),
            t.mean(),
            b.mean()
        );
    };
    let of = |f: &dyn Fn(i32, &glyph_enhance::heros_table::Entry) -> bool| table.glyphs.iter().filter(|(c, e)| f(**c, e)).map(|(c, _)| *c).collect::<Vec<_>>();
    println!("averaged back at 4x (mean over inked pixels of the larger of the premultiplied grey's and the alpha's difference):");
    row("ASCII letters", of(&|c, e| c < 0x80 && matches!(e.class, Some(Class::Letter | Class::Descender))));
    row("ASCII digits", of(&|c, e| c < 0x80 && e.class == Some(Class::Digit)));
    row("ASCII marks", of(&|c, e| c < 0x80 && e.class == Some(Class::Mark)));
    row("Latin-1 and Extended-A", of(&|c, _| (0xa0..0x250).contains(&c)));
    row("Greek", of(&|c, _| (0x370..0x400).contains(&c)));
    row("Cyrillic", of(&|c, _| (0x400..0x500).contains(&c)));
    row("other (symbols, marks)", of(&|c, _| c >= 0x2000 || (0x80..0xa0).contains(&c)));
    row("block 0", of(&|_, _| true));
    for choice in [Choice::Heros, Choice::Traced, Choice::Picture] {
        let list: Vec<String> = table
            .glyphs
            .iter()
            .filter(|(_, e)| e.choice == choice)
            .map(|(c, _)| char::from_u32(*c as u32).map_or(format!("{c:04x}"), |ch| ch.to_string()))
            .collect();
        println!(
            "{} ({}): {}",
            match choice {
                Choice::Heros => "heros",
                Choice::Traced => "traced",
                Choice::Picture => "picture",
            },
            list.len(),
            list.join("")
        );
    }
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&out, table.to_text()).unwrap_or_else(|e| {
        eprintln!("{}: {e}", out.display());
        std::process::exit(1);
    });
    println!("wrote {}", out.display());
}
