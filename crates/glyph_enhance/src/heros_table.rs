use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use scene::font::Font;

use crate::heros::{HEROS_REGULAR, HerosFont};
use crate::heros_fit::{Class, Model, Placement, Target, Zones, fit_model, place};
use crate::outlines::{OutlineFont, block_zero_codes, font_hash};
use crate::picture::{AveragedBack, SharpFont, SharpGlyph, Source, averaged_back};
use crate::trace::Contour;

pub const FORMAT: &str = "crystalosd-heros-fit 1";

// The fit of the bundled FNTOSD (Resources/FNTOSD), made by crystalosd-heros-fit: measurements of where and how the
// console placed each glyph, no picture of it.
pub const BUNDLED: &str = include_str!("../data/heros-fit.txt");

// The decision, by the sharp picture averaged back to the console's pixels (picture::averaged_back at 4 texels a pixel,
// mean over the inked pixels): Heros where it is within MARGIN of the traced outline and within BOUND; else the traced
// outline; else the picture. The traced outline is fitted to the very pixels it is measured on, a font's outline only to
// the shape they were drawn from, so Heros trails it on most glyphs: over the rest of ASCII by -0.01 to 0.03,
// continuously, then by 0.088 to 0.197 on the four glyphs Sony redrew (# % @ ~). MARGIN sits in that gap. A per-glyph
// "at least as good as traced" would only cut that continuum, and mix the two sources inside words.
pub const MARGIN: f64 = 0.05;
pub const BOUND: f64 = 0.10;
pub const MEASURE_SCALE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Heros,
    Traced,
    Picture,
}

impl Choice {
    fn name(self) -> &'static str {
        match self {
            Self::Heros => "heros",
            Self::Traced => "traced",
            Self::Picture => "picture",
        }
    }
}

// One glyph of block 0: its source and why. `core`: the fitted outline through the run's model against the console's core
// (mean absolute coverage difference over the inked pixels); `heros`, `traced`, `bilinear`: each source's sharp picture
// averaged back against the console's picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub choice: Choice,
    pub class: Option<Class>,
    pub placement: Option<Placement>,
    pub core: Option<f64>,
    pub heros: Option<f64>,
    pub traced: Option<f64>,
    pub bilinear: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FitTable {
    pub fntosd: u64,
    pub heros: u64,
    pub models: [Model; 2],
    pub glyphs: BTreeMap<i32, Entry>,
}

pub fn decide(heros: Option<f64>, traced: Option<f64>) -> Choice {
    match (heros, traced) {
        (Some(h), Some(t)) if h <= t + MARGIN && h <= BOUND => Choice::Heros,
        (Some(h), None) if h <= BOUND => Choice::Heros,
        (_, Some(_)) => Choice::Traced,
        _ => Choice::Picture,
    }
}

fn heros_glyph(target: &Target<'_>, zones: &Zones, model: &Model, placement: &Placement, outer: &[Contour]) -> SharpGlyph {
    SharpGlyph {
        source: Source::Heros,
        width: target.width as i32,
        height: target.height as i32,
        core: place(target.outline, zones, model, placement, target.origin_x),
        outer: outer.to_vec(),
    }
}

// The run's index in `models` for a glyph: ASCII was drawn by the first run; the rest by whichever fits it better.
fn runs(code: i32) -> &'static [usize] {
    if code < 0x80 { &[1] } else { &[1, 2] }
}

// Every block-0 glyph fitted, measured and decided, on `threads` threads.
pub fn fit_glyphs(font: &Font, heros: &HerosFont, models: &[Model; 2], traced: &OutlineFont, threads: usize) -> BTreeMap<i32, Entry> {
    let zones = Zones::of(heros);
    let codes = block_zero_codes(font);
    let chunk = codes.len().div_ceil(threads.max(1)).max(1);
    let zones = &zones;
    std::thread::scope(|scope| {
        let handles: Vec<_> = codes
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || part.iter().filter_map(|&code| Some((code, fit_one(font, heros, zones, models, traced, code)?))).collect::<Vec<_>>())
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a fitting thread")).collect()
    })
}

fn fit_one(font: &Font, heros: &HerosFont, zones: &Zones, models: &[Model; 2], traced: &OutlineFont, code: i32) -> Option<Entry> {
    font.glyph(code)?;
    let traced_shape = traced.glyphs.get(&code).map(SharpGlyph::from);
    let measure = |shape: Option<&SharpGlyph>| averaged_back(font, code, shape, MEASURE_SCALE).map_or(f64::INFINITY, |a| a.mean());
    let bilinear = measure(None);
    let traced_error = traced_shape.as_ref().map(|s| measure(Some(s)));
    let target = Target::new(font, heros, code);
    let (class, placement, core, heros_error) = match (&target, &traced_shape) {
        (Some(t), Some(shape)) => {
            let mut best: Option<(Placement, f64)> = None;
            for &run in runs(code) {
                let model = &models[run - 1];
                let p = t.fit(zones, model, run);
                let e = t.mean_error(zones, model, &p);
                if best.is_none_or(|b| e < b.1) {
                    best = Some((p, e));
                }
            }
            let (p, core) = best.expect("a run");
            let glyph = heros_glyph(t, zones, &models[p.run - 1], &p, &shape.outer);
            (Some(t.class), Some(p), Some(core), Some(measure(Some(&glyph))))
        }
        (Some(t), None) => (Some(t.class), None, None, None),
        _ => (None, None, None, None),
    };
    Some(Entry { choice: decide(heros_error, traced_error), class, placement, core, heros: heros_error, traced: traced_error, bilinear })
}

// The two runs and, per run, the training and validation errors (mean over the inked pixels, each glyph at its best
// placement).
pub struct ModelFit {
    pub models: [Model; 2],
    pub errors: [(f64, f64); 2],
}

// The first run fitted on half of the ASCII letters without descenders (every other code) and validated on the other
// half; the second on half of the accented Latin letters, from the first.
pub fn fit_models(font: &Font, heros: &HerosFont, start: &Model, threads: usize) -> ModelFit {
    let zones = Zones::of(heros);
    let codes = block_zero_codes(font);
    let targets: Vec<Target<'_>> = codes.iter().filter_map(|&c| Target::new(font, heros, c)).collect();
    let set = |run: usize| -> Vec<&Target<'_>> {
        targets.iter().filter(|t| t.class == Class::Letter && if run == 1 { t.code < 0x80 } else { (0xc0..0x180).contains(&t.code) }).collect()
    };
    let mean = |set: &[&Target<'_>], model: &Model, run: usize| {
        let (mut sum, mut inked) = (0.0, 0usize);
        for t in set {
            let (s, n) = t.error(&zones, model, &t.fit(&zones, model, run), true);
            sum += s;
            inked += n;
        }
        sum / inked.max(1) as f64
    };
    let mut models = [*start, *start];
    let mut errors = [(0.0, 0.0); 2];
    for run in 1..=2 {
        let all = set(run);
        let train: Vec<&Target<'_>> = all.iter().copied().step_by(2).collect();
        let validate: Vec<&Target<'_>> = all.iter().copied().skip(1).step_by(2).collect();
        let mut model = if run == 1 { *start } else { models[0] };
        for _ in 0..3 {
            model = fit_model(&train, &zones, &model, run, threads, 200);
        }
        errors[run - 1] = (mean(&train, &model, run), mean(&validate, &model, run));
        models[run - 1] = model;
    }
    ModelFit { models, errors }
}

fn number(v: Option<f64>) -> String {
    v.map_or("-".to_owned(), |v| format!("{v:.4}"))
}

impl FitTable {
    pub fn to_text(&self) -> String {
        let mut out = format!("{FORMAT}\nfntosd {:016x}\nheros {:016x}\n", self.fntosd, self.heros);
        for (i, m) in self.models.iter().enumerate() {
            let _ = writeln!(
                out,
                "run {} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {}",
                i + 1,
                m.scale_x,
                m.scale_y,
                m.baseline,
                m.embolden_x,
                m.embolden_y,
                m.blur,
                u8::from(m.remap)
            );
        }
        out.push_str("# code source class run dx dy sx sy descender bold accent-dx accent-dy | core heros traced bilinear\n");
        for (code, e) in &self.glyphs {
            let placement = e.placement.map_or("- - - - - - - - -".to_owned(), |p| {
                format!("{} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} {:.4}", p.run, p.dx, p.dy, p.sx, p.sy, p.descender, p.bold, p.accent[0], p.accent[1])
            });
            let _ = writeln!(
                out,
                "glyph {code:04x} {} {} {placement} | {} {} {} {:.4}",
                e.choice.name(),
                e.class.map_or("-", Class::name),
                number(e.core),
                number(e.heros),
                number(e.traced),
                e.bilinear
            );
        }
        out
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().filter(|l| !l.starts_with('#'));
        if lines.next() != Some(FORMAT) {
            return Err(format!("not a {FORMAT} file"));
        }
        let hash = |line: Option<&str>, key: &str| -> Result<u64, String> {
            line.and_then(|l| l.strip_prefix(key)).and_then(|h| u64::from_str_radix(h.trim(), 16).ok()).ok_or(format!("no {key}"))
        };
        let fntosd = hash(lines.next(), "fntosd ")?;
        let heros = hash(lines.next(), "heros ")?;
        let mut models = [Model { scale_x: 0.0, scale_y: 0.0, baseline: 0.0, embolden_x: 0.0, embolden_y: 0.0, blur: 0.0, remap: false }; 2];
        for m in &mut models {
            let line = lines.next().ok_or("no run")?;
            let v: Vec<f64> = line.split_whitespace().skip(2).map(str::parse).collect::<Result<_, _>>().map_err(|_| format!("a bad run: {line}"))?;
            if v.len() != 7 {
                return Err(format!("a bad run: {line}"));
            }
            *m = Model { scale_x: v[0], scale_y: v[1], baseline: v[2], embolden_x: v[3], embolden_y: v[4], blur: v[5], remap: v[6] != 0.0 };
        }
        let mut glyphs = BTreeMap::new();
        for line in lines {
            let w: Vec<&str> = line.split_whitespace().collect();
            if w.len() != 18 || w[0] != "glyph" || w[13] != "|" {
                return Err(format!("a bad glyph line: {line}"));
            }
            let bad = || format!("a bad glyph line: {line}");
            let code = i32::from_str_radix(w[1], 16).map_err(|_| bad())?;
            let choice = match w[2] {
                "heros" => Choice::Heros,
                "traced" => Choice::Traced,
                "picture" => Choice::Picture,
                _ => return Err(bad()),
            };
            let class = match w[3] {
                "letter" => Some(Class::Letter),
                "descender" => Some(Class::Descender),
                "digit" => Some(Class::Digit),
                "mark" => Some(Class::Mark),
                _ => None,
            };
            let optional = |s: &str| if s == "-" { Ok(None) } else { s.parse::<f64>().map(Some).map_err(|_| bad()) };
            let placement = if w[4] == "-" {
                None
            } else {
                let v: Vec<f64> = w[5..13].iter().map(|s| s.parse::<f64>()).collect::<Result<_, _>>().map_err(|_| bad())?;
                let run = w[4].parse::<usize>().ok().filter(|r| (1..=2).contains(r)).ok_or_else(bad)?;
                Some(Placement { run, dx: v[0], dy: v[1], sx: v[2], sy: v[3], descender: v[4], bold: v[5], accent: [v[6], v[7]] })
            };
            let bilinear = w[17].parse::<f64>().map_err(|_| bad())?;
            glyphs.insert(code, Entry { choice, class, placement, core: optional(w[14])?, heros: optional(w[15])?, traced: optional(w[16])?, bilinear });
        }
        Ok(Self { fntosd, heros, models, glyphs })
    }

    // The shapes `--text sharp` draws: Heros' outline placed as fitted, inside the traced outline's dark outer edge, where
    // the table chose Heros; the traced outlines where it chose them, or for a code it does not list; else the picture.
    pub fn sharp_font(&self, font: &Font, heros: &HerosFont, traced: &OutlineFont) -> SharpFont {
        let zones = Zones::of(heros);
        let mut out = SharpFont::traced(traced);
        for (code, e) in &self.glyphs {
            match e.choice {
                Choice::Heros => {
                    if let (Some(p), Some(t), Some(shape)) = (e.placement, Target::new(font, heros, *code), out.glyphs.get(code)) {
                        let glyph = heros_glyph(&t, &zones, &self.models[p.run - 1], &p, &shape.outer);
                        out.glyphs.insert(*code, glyph);
                    }
                }
                Choice::Traced => {}
                Choice::Picture => {
                    out.glyphs.remove(code);
                }
            }
        }
        out
    }

    pub fn count(&self, choice: Choice) -> usize {
        self.glyphs.values().filter(|e| e.choice == choice).count()
    }
}

pub fn heros_hash() -> u64 {
    font_hash(HEROS_REGULAR)
}

// Where the fit of an FNTOSD other than the bundled one is kept: `directory`/sharp-text/FNTOSD-<hash>.heros-fit.
pub fn cache_path(directory: &Path, file_hash: u64) -> PathBuf {
    directory.join("sharp-text").join(format!("FNTOSD-{file_hash:016x}.heros-fit"))
}

pub enum Found {
    Bundled(FitTable),
    Read(FitTable),
    Fitted(FitTable, Result<(), String>),
}

// The fit of this FNTOSD: the bundled table when it is the bundled font's, else the one kept at `path`, else fitted now
// with the bundled table's runs (the per-glyph fit and decision only) and kept at `path`.
pub fn load_or_fit(font: &Font, file: &[u8], traced: &OutlineFont, heros: &HerosFont, path: &Path) -> Found {
    let hash = font_hash(file);
    let bundled = FitTable::from_text(BUNDLED).expect("the bundled Heros fit");
    if bundled.fntosd == hash && bundled.heros == heros_hash() {
        return Found::Bundled(bundled);
    }
    if let Ok(text) = std::fs::read_to_string(path)
        && let Ok(found) = FitTable::from_text(&text)
        && found.fntosd == hash
        && found.heros == heros_hash()
    {
        return Found::Read(found);
    }
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let table = FitTable { fntosd: hash, heros: heros_hash(), models: bundled.models, glyphs: fit_glyphs(font, heros, &bundled.models, traced, threads) };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(path, table.to_text()))
        .map_err(|e| format!("{}: {e}", path.display()));
    Found::Fitted(table, written)
}

// Every Unicode code of block 0 Heros draws, for parsing only those.
pub fn heros_for(font: &Font) -> HerosFont {
    HerosFont::regular(&block_zero_codes(font))
}

// Over `codes`, each source's averaged-back error summed: the shapes drawn (`shapes`), the traced outlines, the pictures.
pub fn totals(font: &Font, shapes: &SharpFont, traced: &OutlineFont, codes: &[i32]) -> [AveragedBack; 3] {
    let mut out = [AveragedBack::default(); 3];
    for &code in codes {
        let traced_shape = traced.glyphs.get(&code).map(SharpGlyph::from);
        for (i, shape) in [shapes.glyphs.get(&code), traced_shape.as_ref(), None].into_iter().enumerate() {
            if let Some(a) = averaged_back(font, code, shape, MEASURE_SCALE) {
                out[i].add(&a);
            }
        }
    }
    out
}
