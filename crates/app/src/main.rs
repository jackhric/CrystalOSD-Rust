use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use app::boot_chain::BootPhase;
use app::clock_assets::{rod_mesh_of, upload_browser_textures, upload_clock_textures, upload_opening_textures};
use app::debug_panel::{PanelInfo, Resolution};
use app::golden::{Compared, GoldenChecker, GoldenLine, GoldenWriter, hash_bytes, hash_frame};
use app::golden_scenario::{golden_frames, golden_scenario};
use app::host_inputs::{HostOptions, HostZone, NANOS};
use app::input::{Bindings, PadButton, bit_of, feed};
use app::options::{Options, Parsed, parse, strtoull};
use app::png::write_png;
use app::profile::Stage;
use app::sdl::{self, SdlAudio, WindowHandle};
use app::session::{BrowserPhase, Fatal, Session, SessionConfig, Shell, clock_time, console_now};
use app::settings_store::{StoredSettings, read_stored};
use assets::bytes::FNV_OFFSET;
use assets::clock_textures::OPENING_TEXTURES;
use assets::resources::{Asset, AssetSet, decode_folder, extract_bios, folder_sources, user_data_directory};
use audio::clock_sound::{Video, clock_sound_sources, clock_sound_sources_from_decoded};
use audio::live_audio::{LiveAudio, LiveAudioOptions};
use core_log::Log;
use render::{Device, DeviceOptions, NativeOutput, NativeRenderer, vk};
use scene::browser::screen::CardSlot;
use scene::cold_start::ColdAssets;
use scene::console_clock::{rtc_of, zone_of};
use scene::font::Font;
use scene::frame::{TargetName, TextureSet};
use scene::program_image::ProgramImage;
use scene::rods::load_rod_mesh;
use sdl3_sys::events::*;
use sdl3_sys::gamepad::{SDL_CloseGamepad, SDL_Gamepad, SDL_OpenGamepad};
use sdl3_sys::init::{SDL_INIT_GAMEPAD, SDL_INIT_VIDEO, SDL_Init, SDL_Quit};
use sdl3_sys::video::{SDL_CreateWindow, SDL_DestroyWindow, SDL_WINDOW_HIDDEN, SDL_WINDOW_RESIZABLE, SDL_WINDOW_VULKAN, SDL_WindowFlags};

mod panel;

// The folder of raw OSD resource files: --resources, else "resources" beside the executable (where --bios extracts to).
fn resource_folder(options: &Options) -> PathBuf {
    if !options.resources.as_os_str().is_empty() {
        return options.resources.clone();
    }
    let beside = sdl::base_path().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()).join("resources");
    let development = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Resources"));
    if options.bios.as_os_str().is_empty() && !beside.is_dir() && development.is_dir() {
        return development.canonicalize().unwrap_or_else(|_| development.to_path_buf());
    }
    beside
}

fn file_or_empty(path: &Path) -> Vec<u8> {
    if path.is_file() { std::fs::read(path).unwrap_or_default() } else { Vec::new() }
}

// The picture is 640 x 448 (two 224-line fields) at a scale of height / 448; "window" takes the largest that fits.
fn output_for(resolution: Resolution, samples: u32, window: vk::Extent2D, aspect: f32) -> NativeOutput {
    let height = match resolution {
        Resolution::Native => 448,
        Resolution::Double => 896,
        Resolution::Quadruple => 1792,
        Resolution::Window => window.height.min((window.width as f32 / aspect) as u32).max(2),
    };
    NativeOutput { width: ((640 * height + 224) / 448).max(1), height, samples }
}

enum Action {
    Resize(i32, i32),
    Minimize,
    Restore,
    Set(Resolution, u32),
    Shot(String),
    PadDown(u32),
    PadUp(u32),
    AdjustExpected,
    Paused(bool),
    StepOnce,
    Shown(TargetName),
    Time(i32, i32, i32),
    TvAspect(bool),
    LocalTime,
}

struct Timed {
    at: f64,
    act: Action,
}

fn press(script: &mut Vec<Timed>, at: f64, button: PadButton) {
    let bit = bit_of(button);
    script.push(Timed { at, act: Action::PadDown(bit) });
    script.push(Timed { at: at + 0.1, act: Action::PadUp(bit) });
}

fn script_for(options: &mut Options) -> Vec<Timed> {
    use Action::*;
    use PadButton::*;
    let step = |at: f64, act: Action| Timed { at, act };
    let shot = |at: f64, name: &str| Timed { at, act: Shot(name.to_owned()) };
    let mut script = Vec::new();
    if options.smoke {
        script = vec![step(1.0, Resize(800, 600)), step(2.0, Resize(1024, 700)), step(2.5, Minimize), step(3.5, Restore), step(4.5, Resize(1280, 896))];
        options.soak = 5.5;
    } else if options.soak > 0.0 && options.boot {
        script.push(shot(2.0, "boot-opening"));
        script.push(shot(9.0, "boot-clock"));
        press(&mut script, 10.0, Square);
        script.push(shot(12.5, "boot-after-square"));
        press(&mut script, 13.0, Down);
        press(&mut script, 14.0, Cross);
        script.push(shot(17.0, "boot-menu-next"));
        let mut at = 25.0;
        while at < options.soak - 1.0 {
            script.push(shot(at, &format!("boot-t{}", at as i32)));
            at += 10.0;
        }
    } else if options.soak > 0.0 && !options.clock_start {
        script.push(shot(1.5, "main-menu"));
        press(&mut script, 2.0, Down);
        press(&mut script, 3.5, Up);
        press(&mut script, 5.0, Up);
        press(&mut script, 6.5, Triangle);
        press(&mut script, 7.0, Down);
        press(&mut script, 7.25, Up);
        script.push(shot(7.5, "version"));
        press(&mut script, 7.7, Circle);
        press(&mut script, 8.0, Cross);
        press(&mut script, 10.0, Down);
        press(&mut script, 11.5, Cross);
        script.push(shot(17.0, "configuration"));
        press(&mut script, 18.0, Cross);
        press(&mut script, 19.5, Up);
        press(&mut script, 21.0, Cross);
        script.push(step(22.0, AdjustExpected));
        press(&mut script, 23.0, Cross);
        press(&mut script, 24.0, Up);
        press(&mut script, 25.0, Circle);
        for at in [26.0, 27.0, 28.0] {
            press(&mut script, at, Down);
        }
        press(&mut script, 30.0, Cross);
        script.push(shot(32.0, "entry"));
        press(&mut script, 33.0, Right);
        press(&mut script, 34.5, Circle);
        press(&mut script, 36.0, Cross);
        press(&mut script, 38.0, Cross);
        press(&mut script, 40.0, Square);
        script.push(shot(44.0, "clock-alone"));
        script.push(step(45.0, Resize(800, 600)));
        script.push(step(46.0, Set(Resolution::Window, 4)));
        press(&mut script, 48.0, Square);
        script.push(step(50.0, Minimize));
        script.push(step(51.0, Restore));
        script.push(step(52.0, Set(Resolution::Quadruple, 4)));
        press(&mut script, 54.0, Circle);
        script.push(step(55.0, Set(Resolution::Native, 1)));
        script.push(step(56.0, Resize(1280, 896)));
    } else if options.soak > 0.0 {
        script = vec![
            shot(8.0, "native"),
            step(9.0, Resize(800, 600)),
            step(10.0, Set(Resolution::Window, 4)),
            shot(12.0, "window"),
            step(13.0, Minimize),
            step(15.0, Restore),
            step(17.0, Set(Resolution::Double, 8)),
            step(20.0, Set(Resolution::Quadruple, 4)),
            shot(22.0, "x4"),
            step(23.0, Paused(true)),
            step(24.0, StepOnce),
            step(24.5, StepOnce),
            step(25.0, StepOnce),
            step(26.0, Paused(false)),
            step(27.0, Shown(TargetName::RefractionSource)),
            step(29.0, Shown(TargetName::Work)),
            step(31.0, Shown(TargetName::Display)),
            step(32.0, Resize(1600, 1000)),
            step(34.0, Set(Resolution::Native, 2)),
            step(36.0, Time(23, 59, 50)),
            step(38.0, Set(Resolution::Window, 1)),
            step(40.0, TvAspect(false)),
            step(42.0, Resize(1280, 896)),
            step(44.0, TvAspect(true)),
            step(46.0, Set(Resolution::Double, 4)),
            step(49.0, LocalTime),
            step(51.0, Minimize),
            step(52.0, Restore),
            step(53.0, Set(Resolution::Window, 8)),
            step(55.0, Resize(700, 900)),
            step(57.0, Set(Resolution::Native, 1)),
            step(58.0, Resize(1280, 896)),
        ];
    }
    script.sort_by(|l, r| l.at.partial_cmp(&r.at).unwrap_or(std::cmp::Ordering::Equal));
    script
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = run(&args);
    std::process::exit(code);
}

fn run(args: &[String]) -> i32 {
    let (mut options, explicit) = match parse(args) {
        Parsed::Run(options, explicit) => (*options, explicit),
        Parsed::Exit(code) => return code,
    };
    if !options.log_file.as_os_str().is_empty() {
        Log::get().open(&options.log_file);
    }
    let golden = !options.golden.is_empty();
    if golden {
        options.settings = Path::new("out").join("golden").join("no-settings.json");
        options.pal = false;
        options.language = None;
        options.aspect = None;
        options.mute = true;
        options.boot = true;
    }
    if options.settings.as_os_str().is_empty() {
        options.settings = if options.soak > 0.0 || options.smoke {
            Path::new("out").join("golden").join("no-settings.json")
        } else {
            user_data_directory().join("settings.json")
        };
    }
    let stored_at_start = match read_stored(&options.settings) {
        Ok(stored) => stored,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    let save = !golden && !options.no_save && (options.soak == 0.0 || options.settings_given);
    let Some(host_zone) = HostZone::current() else {
        eprintln!("host time zone: tzdb: cannot determine current zone");
        return 1;
    };
    // The clock's assets: the console's raw resource files decoded at each start; loose files only from explicit flags.
    let folder = resource_folder(&options);
    if !options.bios.as_os_str().is_empty() {
        match extract_bios(&options.bios, &folder) {
            Ok(names) => {
                let names: String = names.iter().map(|n| format!(" {n}")).collect();
                println!("bios: {} gives{}, in {}", options.bios.display(), names, folder.display());
            }
            Err(error) => core_log::log!(Error, Assets, "bios: {}", error),
        }
    }
    let mut loaded: Option<AssetSet> = None;
    let begin = Instant::now();
    match folder_sources(&folder).and_then(|sources| if sources.is_empty() { Ok(None) } else { decode_folder(&folder).map(Some) }) {
        Ok(Some(set)) => {
            println!("assets: {} decoded from {} in {:.1} ms", set.assets.len(), folder.display(), begin.elapsed().as_secs_f64() * 1e3);
            loaded = Some(set);
        }
        Ok(None) => {}
        Err(error) => core_log::log!(Error, Assets, "load: {}", error),
    }
    if loaded.is_none() {
        println!("assets: no resource files in {}", folder.display());
    }
    let decoded = loaded.as_ref();
    let source_of = |name: &str| match decoded.and_then(|d| d.find(name).map(|a| d.source_of(a).path.clone())) {
        Some(path) => format!("decoded from {path}"),
        None => String::new(),
    };
    let has = |name: &str| decoded.is_some_and(|d| d.find(name).is_some());

    // Each group from one place: the decoded set, or a loose file named on the command line.
    let use_texture_files = explicit.contains("--textures");
    let texture_source = if use_texture_files {
        format!("PNG files in {}", options.textures.display())
    } else if has("TEXCFLOW") {
        source_of("TEXCFLOW")
    } else {
        "none".to_owned()
    };
    let mesh_asset = decoded.filter(|_| !explicit.contains("--mesh")).and_then(|d| d.find("RODMESH"));
    let cube_asset = decoded.filter(|_| !explicit.contains("--cube-mesh")).and_then(|d| d.find("CUBEMESH"));
    let mesh_source = if mesh_asset.is_some() { source_of("RODMESH") } else { options.mesh.display().to_string() };
    let font_asset = decoded.and_then(|d| d.find("FNTOSD"));
    let program_asset = decoded.and_then(|d| d.find("PROGRAM"));
    let font_loose = explicit.contains("--font");
    let program_loose = explicit.contains("--program");
    let font_file = if font_loose { file_or_empty(&options.font) } else { font_asset.map(|a| a.data.clone()).unwrap_or_default() };
    let mut program_file = if program_loose { file_or_empty(&options.program) } else { program_asset.map(|a| a.data.clone()).unwrap_or_default() };
    if program_loose && !program_file.is_empty() {
        let known = assets::elf_image::ElfImage::new(&program_file).is_ok_and(|elf| assets::program::is_hdd_osd_110u(&elf));
        if !known {
            eprintln!("text: {} is not HDD OSD 1.10U's hddosd.elf: not read", options.program.display());
            program_file.clear();
        }
    }
    let text_source = if font_file.is_empty() || program_file.is_empty() {
        "none (no FNTOSD or no HDD OSD 1.10U hddosd.elf)".to_owned()
    } else {
        format!(
            "{} and {}",
            if font_loose { options.font.display().to_string() } else { source_of("FNTOSD") },
            if program_loose { options.program.display().to_string() } else { source_of("PROGRAM") }
        )
    };
    println!("textures: {texture_source}\nmesh: {mesh_source}\ntext: {text_source}");
    let mut missing = Vec::new();
    if !use_texture_files && !has("TEXCFLOW") {
        missing.push("the clock textures");
    }
    if mesh_asset.is_none() && options.mesh.as_os_str().is_empty() {
        missing.push("the rod mesh");
    }
    if cube_asset.is_none() && options.cube_mesh.as_os_str().is_empty() {
        missing.push("the cube mesh");
    }
    if program_file.is_empty() {
        missing.push("the program hddosd.elf (HDD OSD 1.10U)");
    }
    if options.opening_textures.as_os_str().is_empty() && !has(OPENING_TEXTURES[0].name) {
        missing.push("the opening textures");
    }
    if !missing.is_empty() {
        eprintln!(
            "missing: {}.\nGive the console's files: --bios rom.bin (a PS2 BIOS image) or --resources dir (a folder with TEXIMAGE, FNTOSD, SNDIMAGE and hddosd.elf),\nor put those files in {}.",
            missing.join(", "),
            folder.display()
        );
        return 1;
    }
    // SAFETY: SDL is initialised once, on this thread, and quit at the end of run.
    if !unsafe { SDL_Init(SDL_INIT_VIDEO | SDL_INIT_GAMEPAD) } {
        eprintln!("SDL: {}", sdl::error());
        return 1;
    }
    let title = CString::new("CrystalOSD").expect("no NUL");
    let hidden = golden || !options.browser_shots.as_os_str().is_empty();
    let flags: SDL_WindowFlags = SDL_WINDOW_VULKAN | SDL_WINDOW_RESIZABLE | if hidden { SDL_WINDOW_HIDDEN } else { SDL_WindowFlags(0) };
    // SAFETY: SDL is initialised.
    let window = WindowHandle(unsafe { SDL_CreateWindow(title.as_ptr(), 1280, 896, flags) });
    if window.0.is_null() {
        eprintln!("window: {}", sdl::error());
        return 1;
    }
    let setup = Setup {
        options,
        golden,
        save,
        stored_at_start,
        host_zone,
        folder,
        decoded,
        use_texture_files,
        opening_source: source_of(OPENING_TEXTURES[0].name),
        mesh_asset: mesh_asset.cloned(),
        cube_asset: cube_asset.cloned(),
        font_file,
        program_file,
    };
    let code = match body(setup, window) {
        Ok(code) => code,
        Err(Fatal::DeviceLost(what)) => {
            core_log::log!(Error, Render, "device lost: {}", what);
            2
        }
        Err(Fatal::Error(what)) => {
            core_log::log!(Error, App, "fatal: {}", what);
            1
        }
    };
    // SAFETY: everything made from the window is gone.
    unsafe {
        SDL_DestroyWindow(window.0);
        SDL_Quit();
    }
    code
}

struct Setup<'a> {
    options: Options,
    golden: bool,
    save: bool,
    stored_at_start: StoredSettings,
    host_zone: HostZone,
    folder: PathBuf,
    decoded: Option<&'a AssetSet>,
    use_texture_files: bool,
    opening_source: String,
    mesh_asset: Option<Asset>,
    cube_asset: Option<Asset>,
    font_file: Vec<u8>,
    program_file: Vec<u8>,
}

// --browser-shots dir --browser-script steps.json: the session from the main menu at rest, driven by the steps of the emulator's
// snapshot scripts ({"press": button, "held": n}, {"to": frame}, {"snapshot": name}), rendered headless at 640 x 448 (or
// --browser-shots-output WxHxSAMPLES) and saved as PNG. A press reaches the scene as late as the console's code sees a pad command: the Browser's pad words one pass later (read at
// the pass's start, 0x00253B58), the clock's menus two frames later (its pad update runs after its frame functions,
// facts/sound-events.md section 1); a press advances
// held + 1 frames and a snapshot 2, as watson_pad and watson_snapshot do, so the frame numbers are the emulator's.
fn browser_shots(shell: &mut Shell, renderer: &mut NativeRenderer<'_>, font: Option<&Arc<Font>>, options: &Options, info: &mut PanelInfo) -> Result<(), Fatal> {
    let [width, height, samples] = options.browser_shots_output;
    renderer.configure(&NativeOutput { width, height, samples })?;
    let text = std::fs::read(&options.browser_script).map_err(|e| fatal(format!("{}: {e}", options.browser_script.display())))?;
    let steps: serde_json::Value = serde_json::from_slice(&text).map_err(|e| fatal(format!("{}: {e}", options.browser_script.display())))?;
    let start = shell.session.config.host_zone.to_sys_earliest(2026, 1, 1, 12) * NANOS;
    shell.session.golden_start = start;
    shell.session.fixed_now = Some(start);
    shell.panel.mute = true;
    std::fs::create_dir_all(&options.browser_shots).map_err(fatal)?;
    let mut frame = 0u64;
    let mut pending: Vec<(u64, u32, bool)> = Vec::new();
    let mut phase = String::new();
    let mut advance = |shell: &mut Shell, renderer: &mut NativeRenderer<'_>, pending: &mut Vec<(u64, u32, bool)>, frame: &mut u64| -> Result<(), Fatal> {
        for &(at, bit, down) in pending.iter() {
            if at == *frame {
                if down {
                    shell.session.reader.down(0, bit);
                } else {
                    shell.session.reader.up(0, bit);
                }
            }
        }
        shell.produce()?;
        if let Some(font) = font
            && shell.frame.texture_set != TextureSet::Opening
        {
            renderer.set_glyph_cache(font, &shell.frame.glyphs)?;
            renderer.set_glyph_layouts(font, &shell.frame.glyph_layouts)?;
        }
        renderer.draw(&shell.frame)?;
        let now = format!("{:?}", shell.session.browser_phase);
        if now != phase {
            println!("frame {}: Browser phase {}", *frame, now);
            phase = now;
        }
        *frame += 1;
        Ok(())
    };
    for step in steps.as_array().ok_or_else(|| fatal("the script is not a list"))? {
        if let Some(button) = step.get("press").and_then(|b| b.as_str()) {
            let held = step.get("held").and_then(|h| h.as_u64()).unwrap_or(6);
            let bit = match button {
                "cross" => bit_of(PadButton::Cross),
                "circle" => bit_of(PadButton::Circle),
                "triangle" => bit_of(PadButton::Triangle),
                "square" => bit_of(PadButton::Square),
                "left" => bit_of(PadButton::Left),
                "right" => bit_of(PadButton::Right),
                "up" => bit_of(PadButton::Up),
                "down" => bit_of(PadButton::Down),
                other => return Err(fatal(format!("unknown button {other}"))),
            };
            let latency = if shell.session.browser_phase == BrowserPhase::Running { 1 } else { 2 };
            pending.push((frame + latency, bit, true));
            pending.push((frame + latency + held, bit, false));
            for _ in 0..=held {
                advance(shell, renderer, &mut pending, &mut frame)?;
            }
        } else if let Some(to) = step.get("to").and_then(|t| t.as_u64()) {
            while frame < to {
                advance(shell, renderer, &mut pending, &mut frame)?;
            }
        } else if let Some(name) = step.get("snapshot").and_then(|n| n.as_str()) {
            let stem = Path::new(name).file_stem().map_or(name.to_owned(), |s| s.to_string_lossy().into_owned());
            shoot(renderer, TargetName::Display, &stem, &options.browser_shots, info)?;
            for _ in 0..2 {
                advance(shell, renderer, &mut pending, &mut frame)?;
            }
        }
    }
    Ok(())
}

// --memory-card-1 / --memory-card-2: a raw card image (facts/memory-card-image.md), read only. A formatted image is a card of type 2
// with the free count the console's library gives for it (assets::memory_card::library_free), the Browser shows max(free - 2, 0) KB.
fn memory_cards(files: &[PathBuf; 2]) -> Result<[Option<CardSlot>; 2], Fatal> {
    let mut out = [None, None];
    for (n, file) in files.iter().enumerate() {
        if file.as_os_str().is_empty() {
            continue;
        }
        let image = assets::memory_card::read_image(file).map_err(|e| fatal(e.0))?;
        let free = assets::memory_card::library_free(&image);
        let slot = CardSlot { formatted: free.is_some(), free: free.unwrap_or(0) };
        match free {
            Some(f) => println!("memory card {}: {}: formatted, free {} clusters, the Browser shows {} KB Free", n + 1, file.display(), f, (f - 2).max(0)),
            None => println!("memory card {}: {}: unformatted", n + 1, file.display()),
        }
        out[n] = Some(slot);
    }
    Ok(out)
}

fn fatal(what: impl ToString) -> Fatal {
    Fatal::Error(what.to_string())
}

fn shoot(renderer: &mut NativeRenderer<'_>, shown: TargetName, name: &str, screenshots: &Path, info: &mut PanelInfo) -> Result<(), Fatal> {
    let out = *renderer.output();
    let mut rgba = renderer.read_target(shown)?;
    for alpha in rgba.iter_mut().skip(3).step_by(4) {
        *alpha = 255;
    }
    let path = screenshots.join(format!("{name}-{}x{}-msaa{}.png", out.width, out.height, out.samples));
    write_png(&path, out.width, out.height, &rgba).map_err(fatal)?;
    info.last_screenshot = path.display().to_string();
    println!("screenshot: {}", info.last_screenshot);
    Ok(())
}

fn body(setup: Setup<'_>, window: WindowHandle) -> Result<i32, Fatal> {
    let Setup {
        mut options,
        golden,
        save,
        stored_at_start,
        host_zone,
        folder,
        decoded,
        use_texture_files,
        opening_source,
        mesh_asset,
        cube_asset,
        font_file,
        program_file,
    } = setup;
    let mut code = 0;
    let device = Device::new(Some(Box::new(window)), &DeviceOptions { validation: options.validation, sync_validation: options.sync_validation })?;
    let mut renderer = NativeRenderer::new(&device, &options.shaders)?;
    if use_texture_files {
        renderer.load_clock_textures(&options.textures)?;
    } else {
        upload_clock_textures(&mut renderer, decoded.expect("decoded assets")).map_err(fatal)?;
    }
    {
        let opening_files = !options.opening_textures.as_os_str().is_empty();
        if opening_files {
            renderer.load_opening_textures(&options.opening_textures)?;
        } else {
            upload_opening_textures(&mut renderer, decoded.expect("decoded assets")).map_err(fatal)?;
        }
        println!("opening textures: {}", if opening_files { format!("PNG files in {}", options.opening_textures.display()) } else { opening_source });
    }

    // The clock starts as the console's does: the program's initial values, the init functions, the host's inputs (scene/ColdStart.cpp).
    let cube_mesh = match &cube_asset {
        Some(asset) => rod_mesh_of(asset).map_err(fatal)?,
        None => load_rod_mesh(&options.cube_mesh).map_err(fatal)?,
    };
    let program = Arc::new(ProgramImage::new(program_file).map_err(fatal)?);
    let font = if !font_file.is_empty() {
        Some(Arc::new(Font::new(&font_file).map_err(fatal)?))
    } else {
        println!("no FNTOSD: the clock runs without text");
        None
    };
    let mesh = match &mesh_asset {
        Some(asset) => rod_mesh_of(asset).map_err(fatal)?,
        None => load_rod_mesh(&options.mesh).map_err(fatal)?,
    };
    if options.text_sharp
        && let Some(font) = &font
    {
        app::sharp_text::enable(&mut renderer, font, &font_file, &options.text_outlines);
    }
    let cold_assets = ColdAssets { program: program.clone(), font: font.clone(), mesh, cube_mesh };
    match assets::resources::decode_browser_sheets(&folder) {
        Ok(sheets) => upload_browser_textures(&mut renderer, &sheets).map_err(fatal)?,
        Err(error) => core_log::log!(Warn, App, "Browser sheets: {}", error.0),
    }
    let memory_cards = memory_cards(&options.memory_cards)?;

    let capture_all = options.capture == "all";
    let mut capture_at = BTreeSet::new();
    let mut at = 0;
    while at < options.capture.len() && !capture_all {
        capture_at.insert(strtoull(&options.capture[at..], 10));
        let Some(comma) = options.capture[at..].find(',') else { break };
        at += comma + 1;
    }
    let capturing = options.boot && !options.capture.is_empty();

    let mut gui = if golden { None } else { Some(panel::Gui::new(window, &device)?) };

    let mut info = PanelInfo { sample_counts: device.sample_counts().as_raw(), ..Default::default() };
    let config = SessionConfig {
        host: HostOptions { pal: options.pal, language: options.language, aspect: options.aspect },
        host_zone,
        cold_assets,
        program,
        pass_names: device.labels() || !options.trace_file.as_os_str().is_empty(),
        save,
        settings: options.settings.clone(),
        lights_phase: options.lights_phase,
        towers_demo: options.towers_demo,
        browser: !golden,
        memory_cards,
    };
    let mut shell = Shell::new(Session::new(config, stored_at_start));
    shell.panel.mute = options.mute;
    shell.profiling = options.profile;
    let bindings = Bindings::defaults();
    let mut gamepads: BTreeMap<u32, *mut SDL_Gamepad> = BTreeMap::new();
    {
        let audio_options = LiveAudioOptions {
            mute: options.mute,
            device: !golden,
            video: if options.pal { Video::Pal } else { Video::Ntsc },
            wav: options.audio_wav.clone(),
            command_log: if options.audio_wav.as_os_str().is_empty() {
                PathBuf::new()
            } else {
                PathBuf::from(format!("{}.commands.txt", options.audio_wav.display()))
            },
        };
        let sources = match decoded {
            Some(d) if d.find("SNDIMAGE").is_some() => clock_sound_sources_from_decoded(d),
            _ => clock_sound_sources(&folder),
        };
        match sources.and_then(|s| LiveAudio::with_device(s, &audio_options, SdlAudio::open)) {
            Ok(live) => {
                shell.session.audio = Some(live);
                core_log::log!(Info, Audio, "init: ready, mute {}", options.mute);
            }
            Err(error) => core_log::log!(Error, Audio, "off: {}", error),
        }
    }
    if !options.trace_file.as_os_str().is_empty() {
        shell.trace = std::fs::File::create(&options.trace_file).ok();
    }
    if golden {
        let start = shell.session.config.host_zone.to_sys_earliest(2026, 1, 1, 12) * NANOS;
        shell.session.golden_start = start;
        shell.session.fixed_now = Some(start);
        shell.panel.mute = true;
    }
    if options.boot {
        shell.build_boot();
    } else {
        let s = &mut shell.session;
        s.start_clock(false, None)?;
        s.settle(|s| s.clock().state().mode == 0 && s.clock().menus().expect("menus").main_menu.ramp.state == 2);
        if options.clock_start {
            let run = |s: &mut Session, count: i32| {
                let mut n = 0;
                s.settle(|_| {
                    let done = n >= count;
                    n += 1;
                    done
                });
            };
            let tap = |s: &mut Session, button: PadButton| {
                let bit = bit_of(button);
                s.reader.down(0, bit);
                run(s, 6);
                s.reader.up(0, bit);
                run(s, 1);
            };
            tap(s, PadButton::Down);
            tap(s, PadButton::Cross);
            s.settle(|s| s.clock().menus().expect("menus").page.ramp.state == 2);
            tap(s, PadButton::Square);
            s.settle(|s| s.clock().state().menu_ramp.state == 2);
            if let Some(audio) = &mut s.audio {
                audio.discard_pending();
            }
        }
        s.clock_thread_starts();
        let state = s.clock().state();
        println!("start: frame {}, mode {}, menu ramp state {}", state.counter, state.mode, state.menu_ramp.state);
    }
    if !golden {
        shell.produce()?;
    }

    let mut screenshot_name = String::new();
    let script = script_for(&mut options);
    let mut next = 0;

    let step = 1001.0 / 60000.0;
    // One present per logic frame, the loop sleeping until the next step is due: FIFO then shows every logic
    // frame for the same number of refreshes. Presenting as fast as the loop wakes (2 frames in flight, so in
    // bursts) showed the 59.94 Hz frames for uneven runs of refreshes, a judder.
    let mut owed = 0.0f64;
    let begun = sdl::ticks_ns();
    let (mut last, mut last_present) = (begun, begun);
    let mut presented = 0u64;
    // Intervals between presents of consecutive logic frames, for the cadence report.
    let (mut intervals, mut uneven) = (0u64, 0u64);
    let (mut interval_sum, mut interval_worst) = (0.0f64, 0.0f64);
    let mut fps = 0f32;
    if golden {
        let custom = app::options::custom_output(&options.golden_output);
        renderer.configure(&if let Some((width, height, samples)) = custom {
            NativeOutput { width, height, samples }
        } else if options.golden_output == "x2-msaa4" {
            NativeOutput { width: 1280, height: 896, samples: 4 }
        } else {
            NativeOutput { width: 640, height: 448, samples: 1 }
        })?;
        // Sharp text and outputs other than the baselines' are not the console's picture: their scene and sound are checked.
        let pixels_compared = !options.text_sharp && custom.is_none();
        if !pixels_compared && options.golden == "record" {
            println!("golden: a record is the console's picture: not with --text sharp or a custom --golden-output");
            return Ok(1);
        }
        if !pixels_compared {
            println!("golden: pixels not compared ({}), scene and sound are", if options.text_sharp { "--text sharp" } else { "a custom --golden-output" });
        }
        let mut writer = None;
        let mut checker = None;
        if options.golden == "record" {
            writer = Some(GoldenWriter::new(&options.golden_file).map_err(|_| fatal(format!("golden: cannot write {}", options.golden_file.display())))?);
        } else {
            checker = Some(GoldenChecker::new(&options.golden_file, Compared { pixels: pixels_compared, ..Compared::ALL }));
        }
        let presses = golden_scenario();
        let mut clock_frames = 0u64;
        const GOLDEN_CAP: u64 = 4000;
        let mut identical = true;
        while clock_frames < golden_frames() {
            if shell.logic_frames >= GOLDEN_CAP {
                identical = false;
                println!("golden: DIFFERENT - the boot never reached the clock in {GOLDEN_CAP} frames");
                break;
            }
            for press in presses {
                if press.after_clock == clock_frames {
                    shell.session.reader.down(0, bit_of(press.button));
                }
                if press.after_clock + 6 == clock_frames {
                    shell.session.reader.up(0, bit_of(press.button));
                }
            }
            shell.produce()?;
            if let Some(font) = &font
                && shell.frame.texture_set != TextureSet::Opening
            {
                renderer.set_glyph_cache(font, &shell.frame.glyphs)?;
                renderer.set_glyph_layouts(font, &shell.frame.glyph_layouts)?;
            }
            renderer.draw(&shell.frame)?;
            let mut line = GoldenLine {
                frame: shell.logic_frames,
                phase: shell.chain.as_ref().map_or("clock", |c| c.name()).to_owned(),
                screen: shell.session.visited.last().cloned().unwrap_or_default(),
                scene: hash_frame(&shell.frame),
                ..Default::default()
            };
            let pixels = renderer.read_target(TargetName::Display)?;
            line.pixels = hash_bytes(&pixels, FNV_OFFSET);
            line.sound = shell.sound_hash();
            if capture_at.contains(&shell.logic_frames) {
                let name = format!("golden-{:04}-{}", shell.logic_frames, if options.text_sharp { "sharp" } else { "original" });
                shoot(&mut renderer, TargetName::Display, &name, &options.screenshots, &mut info)?;
            }
            if let Some(writer) = &mut writer {
                let _ = writer.add(&line);
            } else if let Some(checker) = &mut checker
                && !checker.check(&line)
            {
                println!("{}", checker.report());
                identical = false;
                break;
            }
            if shell.chain.as_ref().is_none_or(|c| c.phase() == BootPhase::Clock) {
                clock_frames += 1;
            }
        }
        if let Some(checker) = &checker
            && identical
            && checker.compared() != checker.size()
        {
            identical = false;
            println!("golden: DIFFERENT - file has {} frames, run compared {}", checker.size(), checker.compared());
        }
        println!(
            "golden: {} frames {}\nscreens visited: {}",
            checker.as_ref().map_or(shell.logic_frames, |c| c.compared()),
            if !identical {
                "DIFFERENT"
            } else if writer.is_some() {
                "recorded"
            } else {
                "identical"
            },
            shell.session.visited.join(", ")
        );
        if !identical {
            code = 1;
        }
        if device.validation_errors() != 0 {
            println!("golden: {} validation errors", device.validation_errors());
            code = 1;
        }
    }
    if !options.browser_shots.as_os_str().is_empty() {
        browser_shots(&mut shell, &mut renderer, font.as_ref(), &options, &mut info)?;
        return Ok(code);
    }
    let mut running = !golden;
    while running {
        let mut event = SDL_Event::default();
        // SAFETY: SDL is initialised; the event is plain data.
        while unsafe { SDL_PollEvent(&mut event) } {
            if let Some(gui) = &mut gui {
                gui.process_event(&event);
            }
            // SAFETY: every variant starts with the type word.
            let kind = SDL_EventType(unsafe { event.r#type });
            if kind == SDL_EVENT_GAMEPAD_ADDED {
                // SAFETY: a gamepad device event; SDL is initialised.
                let which = unsafe { event.gdevice.which };
                let pad = unsafe { SDL_OpenGamepad(which) };
                if !pad.is_null() {
                    gamepads.insert(which.0, pad);
                }
            }
            if !gui.as_ref().is_some_and(|g| g.wants_keyboard()) || kind != SDL_EVENT_KEY_DOWN {
                feed(&mut shell.session.reader, &bindings, &event);
            }
            if kind == SDL_EVENT_GAMEPAD_REMOVED {
                // SAFETY: a gamepad device event.
                let which = unsafe { event.gdevice.which };
                if let Some(pad) = gamepads.remove(&which.0) {
                    // SAFETY: a gamepad this loop opened.
                    unsafe { SDL_CloseGamepad(pad) };
                }
            }
            if kind == SDL_EVENT_QUIT || kind == SDL_EVENT_WINDOW_CLOSE_REQUESTED {
                running = false;
            }
        }
        let now = sdl::ticks_ns();
        let elapsed = (now - begun) as f64 * 1e-9;
        let delta = (now - last) as f64 * 1e-9;
        last = now;
        if delta > 0.0 {
            fps = fps * 0.95 + (1.0 / delta) as f32 * 0.05;
        }
        if options.soak > 0.0 {
            while next < script.len() && elapsed >= script[next].at {
                let panel = &mut shell.panel;
                match &script[next].act {
                    Action::Resize(w, h) => window.set_size(*w, *h),
                    Action::Minimize => window.minimize(),
                    Action::Restore => window.restore(),
                    Action::Set(r, samples) => {
                        panel.resolution = *r;
                        panel.samples = *samples;
                    }
                    Action::Shot(name) => screenshot_name = name.clone(),
                    Action::PadDown(bit) => shell.session.reader.down(0, *bit),
                    Action::PadUp(bit) => shell.session.reader.up(0, *bit),
                    Action::AdjustExpected => shell.session.adjust_expected = true,
                    Action::Paused(p) => panel.paused = *p,
                    Action::StepOnce => panel.step = true,
                    Action::Shown(t) => panel.shown = *t,
                    Action::Time(h, m, s) => {
                        panel.time = [*h, *m, *s];
                        panel.set_time = true;
                    }
                    Action::TvAspect(t) => panel.tv_aspect = *t,
                    Action::LocalTime => panel.local_time = true,
                }
                next += 1;
            }
            if elapsed >= options.soak {
                running = false;
            }
        }

        if shell.panel.set_time {
            let s = &mut shell.session;
            let mut wanted = console_now(s.wall_now() + s.offset_ns, s.zone_word()).local;
            wanted[3] = shell.panel.time[0];
            wanted[4] = shell.panel.time[1];
            wanted[5] = shell.panel.time[2];
            let rtc = rtc_of(&wanted, zone_of(s.zone_word()));
            s.set_console_to(&rtc);
            shell.panel.set_time = false;
        }
        if shell.panel.local_time {
            shell.session.offset_ns = 0;
            shell.panel.local_time = false;
        }
        if shell.panel.paused {
            owed = 0.0;
            if shell.panel.step {
                shell.produce()?;
            }
        } else if capturing {
            if !shell.fresh {
                shell.produce()?;
            }
        } else {
            owed += delta.min(0.25);
            let mut n = 0;
            while owed >= step {
                owed -= step;
                if n < 4 {
                    shell.produce()?;
                }
                n += 1;
            }
        }
        shell.panel.step = false;
        let since_present = (now - last_present) as f64 * 1e-9;
        if !capturing && (if shell.panel.paused { since_present < step } else { !shell.fresh }) {
            let wait = if shell.panel.paused { step - since_present } else { step - owed };
            sdl::delay_precise((wait.max(0.0) * 1e9) as u64);
            continue;
        }

        let s = &shell.session;
        let shown_time = clock_time(s.wall_now() + s.offset_ns, s.zone_word());
        info.clock = format!("{:02}:{:02}:{:02}", shown_time.hours, shown_time.minutes, shown_time.seconds);
        if let Some(chain) = shell.chain.as_ref().filter(|c| c.phase() != BootPhase::Clock) {
            let opening = chain.opening();
            info.screen = chain.name().to_owned();
            info.counter = opening.counter();
            info.stage = opening.stage();
            info.camera_z = opening.camera_z();
            if let Some(h) = opening.hand_off() {
                info.hand_off = format!("module {}, execute type {}", h.module, h.execute_app_type);
            }
            info.sounds = opening.sounds().len();
        }
        info.logic_frames = shell.logic_frames;
        info.phase = shell.chain.as_ref().map_or("clock", |c| c.name()).to_owned();
        info.last_error = Log::get().last_error();
        info.audio_available = s.audio.is_some();
        if let Some(audio) = &s.audio {
            let a = audio.stats();
            info.audio = format!(
                "{}, queue {}..{} frames, {} underruns, {} overruns, peak {}",
                if a.failed {
                    "stopped"
                } else if a.device_open {
                    "device open"
                } else {
                    "no device"
                },
                if a.min_queued_frames == u32::MAX { 0 } else { a.min_queued_frames },
                a.max_queued_frames,
                a.underruns,
                a.overruns,
                a.peak
            );
        }
        if shell.chain.as_ref().is_none_or(|c| c.phase() == BootPhase::Clock) {
            info.screen = s.visited.last().cloned().unwrap_or_default();
        }
        info.pad = s.inputs.menu.pad;
        info.frames_per_second = fps;
        info.validation_errors = device.validation_errors();
        if let Some(gui) = &mut gui {
            gui.frame(&mut shell.panel, &info);
        }

        if let Some(font) = &font
            && shell.frame.texture_set != TextureSet::Opening
        {
            renderer.set_glyph_cache(font, &shell.frame.glyphs)?;
            renderer.set_glyph_layouts(font, &shell.frame.glyph_layouts)?;
        }
        let Some(context) = device.begin_frame()? else {
            sdl::delay(10);
            continue;
        };
        let aspect = if shell.panel.tv_aspect { 4.0f32 / 3.0 } else { 640.0f32 / 448.0 };
        let output = output_for(shell.panel.resolution, shell.panel.samples, device.swapchain_extent(), aspect);
        if output != *renderer.output() {
            renderer.configure(&output)?;
            shell.fresh = true;
        }
        info.output_width = output.width;
        info.output_height = output.height;
        let profile_screen = if options.profile { shell.screen_key() } else { String::new() };
        if shell.fresh {
            let record_start = sdl::ticks_ns();
            renderer.record(context.cmd, &shell.frame)?;
            if options.profile {
                shell.profile.add(&profile_screen, Stage::Record, (sdl::ticks_ns() - record_start) as f64 * 1e-6);
            }
        }
        let present_start = sdl::ticks_ns();
        let new_frame = shell.fresh;
        shell.fresh = false;
        renderer.present(context.cmd, context.image, device.swapchain_extent(), shell.panel.shown, aspect)?;
        if let Some(gui) = &mut gui {
            gui.render(&device, &context)?;
        }
        device.end_frame(&context)?;
        if options.profile {
            shell.profile.add(&profile_screen, Stage::Present, (sdl::ticks_ns() - present_start) as f64 * 1e-6);
        }
        presented += 1;
        let shown = now;
        if new_frame && !shell.panel.paused && presented > 1 {
            let interval = (shown - last_present) as f64 * 1e-9;
            intervals += 1;
            interval_sum += interval;
            interval_worst = interval_worst.max((interval - step).abs());
            uneven += u64::from((interval - step).abs() > 0.002);
        }
        last_present = shown;

        if capturing {
            if capture_all || capture_at.contains(&shell.logic_frames) {
                let name = format!("boot-{:03}", shell.logic_frames);
                shoot(&mut renderer, shell.panel.shown, &name, &options.screenshots, &mut info)?;
            }
            if shell.logic_frames >= 246 + if capture_all { 40 } else { 0 } {
                running = false;
            }
        }
        if shell.panel.skip_opening && shell.chain.as_ref().is_some_and(|c| c.phase() != BootPhase::Clock) {
            shell.panel.skip_opening = false;
            shell.chain = None;
            shell.session.start_clock(false, None)?;
            shell.session.clock_thread_starts();
            shell.produce()?;
        }
        if shell.panel.restart_opening {
            shell.panel.restart_opening = false;
            shell.session.visited.clear();
            shell.last_phase = BootPhase::Opening;
            shell.build_boot();
            shell.logic_frames = 0;
            shell.produce()?;
        }
        if shell.panel.screenshot || !screenshot_name.is_empty() {
            let name = if screenshot_name.is_empty() { "clock".to_owned() } else { std::mem::take(&mut screenshot_name) };
            shoot(&mut renderer, shell.panel.shown, &name, &options.screenshots, &mut info)?;
            shell.panel.screenshot = false;
        }
    }
    if let Some(audio) = &mut shell.session.audio {
        println!("{}", audio.finish());
    }
    // SAFETY: a live device.
    let _ = unsafe { device.device().device_wait_idle() };
    if let Some(gui) = gui.take() {
        gui.shutdown(&device);
    }
    if options.soak > 0.0 {
        println!(
            "{}: {:.1} s, {} frames presented, {} logic frames, {} validation errors",
            if options.smoke { "smoke" } else { "soak" },
            options.soak,
            presented,
            shell.logic_frames,
            device.validation_errors()
        );
        println!(
            "display {:.2} Hz; {} present intervals, mean {:.3} ms (step {:.3} ms), {} off by more than 2 ms, worst off by {:.3} ms",
            window.refresh_rate(),
            intervals,
            if intervals != 0 { interval_sum / intervals as f64 * 1e3 } else { 0.0 },
            step * 1e3,
            uneven,
            interval_worst * 1e3
        );
    }
    if options.soak > 0.0 && shell.session.clock.as_ref().is_some_and(|c| c.menus().is_some()) {
        println!("screens visited: {}", shell.session.visited.join(", "));
        if shell.session.soak_failed {
            eprintln!("soak: mode 3, leaving or screen code 9999 seen");
            code = 1;
        }
        if shell.session.adjust_expected && (shell.session.adjust_writes != 1 || shell.session.adjust_failed) {
            eprintln!(
                "soak: Clock Adjustment wrote {} times (one expected, none for the cancel) or the edited year was not shown next frame",
                shell.session.adjust_writes
            );
            code = 1;
        }
    }
    if options.profile {
        print!("{}", shell.profile.table());
    }
    if device.validation_errors() != 0 {
        code = 1;
    }
    Ok(code)
}
