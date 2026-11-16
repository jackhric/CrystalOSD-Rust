use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Options {
    pub validation: bool,
    pub sync_validation: bool,
    pub smoke: bool,
    pub soak: f64,
    pub profile: bool,
    pub boot: bool,
    pub towers_demo: bool,
    pub lights_phase: Option<u32>,
    pub opening_textures: PathBuf,
    pub capture: String,
    pub shaders: PathBuf,
    pub textures: PathBuf,
    pub settings: PathBuf,
    pub settings_given: bool,
    pub no_save: bool,
    pub clock_start: bool,
    pub pal: bool,
    pub language: Option<i32>,
    pub aspect: Option<i32>,
    pub cube_mesh: PathBuf,
    pub mesh: PathBuf,
    pub screenshots: PathBuf,
    pub font: PathBuf,
    pub program: PathBuf,
    pub resources: PathBuf,
    pub bios: PathBuf,
    pub mute: bool,
    pub audio_wav: PathBuf,
    pub log_file: PathBuf,
    pub trace_file: PathBuf,
    pub golden: String,
    pub golden_file: PathBuf,
    pub golden_output: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            validation: true,
            sync_validation: false,
            smoke: false,
            soak: 0.0,
            profile: false,
            boot: true,
            towers_demo: false,
            lights_phase: None,
            opening_textures: PathBuf::new(),
            capture: String::new(),
            shaders: PathBuf::from(render::SHADER_DIRECTORY),
            textures: PathBuf::new(),
            settings: PathBuf::new(),
            settings_given: false,
            no_save: false,
            clock_start: false,
            pal: false,
            language: None,
            aspect: None,
            cube_mesh: PathBuf::new(),
            mesh: PathBuf::new(),
            screenshots: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../out/screenshots")),
            font: PathBuf::new(),
            program: PathBuf::new(),
            resources: PathBuf::new(),
            bios: PathBuf::new(),
            mute: false,
            audio_wav: PathBuf::new(),
            log_file: PathBuf::new(),
            trace_file: PathBuf::new(),
            golden: String::new(),
            golden_file: PathBuf::new(),
            golden_output: "native".to_owned(),
        }
    }
}

pub const USAGE: &str = "usage: CrystalOSD [--resources dir] [--bios rom.bin] [--smoke] [--soak seconds] [--skip-boot] [--clock] [--pal] [--language N] [--aspect N] [--towers none|demo] [--lights-phase N] [--opening-textures dir] [--capture all|n,n,...] [--mute] [--audio-wav out.wav] [--log file] [--trace file.jsonl] [--profile] [--golden record|check file] [--golden-output native|x2-msaa4] [--no-validation] [--validate sync] [--shaders dir]\n                    [--settings settings.json] [--no-save] [--screenshots dir] [--textures dir] [--mesh rod-mesh.json] [--cube-mesh cube-mesh.json] [--font FNTOSD] [--program hddosd.elf]\n";

fn skip_space(s: &[u8]) -> usize {
    s.iter().take_while(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')).count()
}

pub fn strtoull(text: &str, base: u32) -> u64 {
    let s = text.as_bytes();
    let mut at = skip_space(s);
    let mut negative = false;
    if at < s.len() && (s[at] == b'+' || s[at] == b'-') {
        negative = s[at] == b'-';
        at += 1;
    }
    let mut base = base;
    let hex_prefix = s.get(at) == Some(&b'0') && matches!(s.get(at + 1), Some(b'x' | b'X')) && s.get(at + 2).is_some_and(|c| c.is_ascii_hexdigit());
    if (base == 0 || base == 16) && hex_prefix {
        at += 2;
        base = 16;
    } else if base == 0 {
        base = if s.get(at) == Some(&b'0') { 8 } else { 10 };
    }
    let mut value = 0u64;
    let mut overflow = false;
    while let Some(digit) = s.get(at).and_then(|&c| (c as char).to_digit(base)) {
        match value.checked_mul(u64::from(base)).and_then(|v| v.checked_add(u64::from(digit))) {
            Some(v) => value = v,
            None => overflow = true,
        }
        at += 1;
    }
    if overflow {
        return u64::MAX;
    }
    if negative { value.wrapping_neg() } else { value }
}

pub fn atoi(text: &str) -> i32 {
    let s = text.as_bytes();
    let mut at = skip_space(s);
    let mut negative = false;
    if at < s.len() && (s[at] == b'+' || s[at] == b'-') {
        negative = s[at] == b'-';
        at += 1;
    }
    let mut value: i128 = 0;
    while let Some(c) = s.get(at).filter(|c| c.is_ascii_digit()) {
        value = (value * 10 + i128::from(c - b'0')).min(i128::from(i64::MAX) + 1);
        at += 1;
    }
    let value = if negative { -value } else { value };
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64 as i32
}

pub fn atof(text: &str) -> f64 {
    let s = &text[skip_space(text.as_bytes())..];
    let b = s.as_bytes();
    let mut end = 0;
    if end < b.len() && (b[end] == b'+' || b[end] == b'-') {
        end += 1;
    }
    let rest = s[end..].to_ascii_lowercase();
    for word in ["infinity", "inf", "nan"] {
        if rest.starts_with(word) {
            return s[..end + word.len()].parse().unwrap_or(0.0);
        }
    }
    let digits_start = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    let mut mantissa = end > digits_start;
    if end < b.len() && b[end] == b'.' {
        let dot = end;
        end += 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        mantissa |= end > dot + 1;
        if !mantissa {
            return 0.0;
        }
    }
    if !mantissa {
        return 0.0;
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut e = end + 1;
        if e < b.len() && (b[e] == b'+' || b[e] == b'-') {
            e += 1;
        }
        if e < b.len() && b[e].is_ascii_digit() {
            while e < b.len() && b[e].is_ascii_digit() {
                e += 1;
            }
            end = e;
        }
    }
    s[..end].parse().unwrap_or(0.0)
}

pub enum Parsed {
    Run(Box<Options>, BTreeSet<String>),
    Exit(i32),
}

pub fn parse(args: &[String]) -> Parsed {
    let mut options = Options::default();
    let mut explicit = BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        let more = i + 1 < args.len();
        explicit.insert(arg.to_owned());
        macro_rules! next {
            () => {{
                i += 1;
                args[i].clone()
            }};
        }
        match arg {
            "--smoke" => options.smoke = true,
            "--skip-boot" => options.boot = false,
            "--towers" if more => options.towers_demo = next!() == "demo",
            "--lights-phase" if more => options.lights_phase = Some(strtoull(&next!(), 0) as u32),
            "--opening-textures" if more => options.opening_textures = next!().into(),
            "--capture" if more => options.capture = next!(),
            "--no-validation" => options.validation = false,
            "--validate" if more => {
                let mode = next!();
                if mode == "sync" {
                    options.sync_validation = true;
                } else {
                    eprintln!("--validate: unknown mode {mode} (sync)");
                    return Parsed::Exit(1);
                }
            }
            "--soak" if more => options.soak = atof(&next!()),
            "--shaders" if more => options.shaders = next!().into(),
            "--textures" if more => options.textures = next!().into(),
            "--settings" if more => {
                options.settings = next!().into();
                options.settings_given = true;
            }
            "--no-save" => options.no_save = true,
            "--clock" => {
                options.clock_start = true;
                options.boot = false;
            }
            "--pal" => options.pal = true,
            "--language" if more => options.language = Some(atoi(&next!())),
            "--aspect" if more => options.aspect = Some(atoi(&next!())),
            "--cube-mesh" if more => options.cube_mesh = next!().into(),
            "--mesh" if more => options.mesh = next!().into(),
            "--screenshots" if more => options.screenshots = next!().into(),
            "--font" if more => options.font = next!().into(),
            "--program" if more => options.program = next!().into(),
            "--resources" if more => options.resources = next!().into(),
            "--bios" if more => options.bios = next!().into(),
            "--mute" => options.mute = true,
            "--audio-wav" if more => options.audio_wav = next!().into(),
            "--log" if more => options.log_file = next!().into(),
            "--profile" => options.profile = true,
            "--trace" if more => options.trace_file = next!().into(),
            "--golden" if i + 2 < args.len() && (args[i + 1] == "record" || args[i + 1] == "check") => {
                options.golden = args[i + 1].clone();
                options.golden_file = args[i + 2].clone().into();
                i += 2;
            }
            "--golden-output" if more && (args[i + 1] == "native" || args[i + 1] == "x2-msaa4") => options.golden_output = next!(),
            _ => {
                eprint!("{USAGE}");
                return Parsed::Exit(1);
            }
        }
        i += 1;
    }
    Parsed::Run(Box::new(options), explicit)
}
