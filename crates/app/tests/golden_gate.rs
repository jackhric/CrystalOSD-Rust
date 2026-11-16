use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn resources() -> Option<PathBuf> {
    let folder = std::env::var_os("CRYSTALOSD_RESOURCES").map(PathBuf::from).unwrap_or_else(|| root().join("../CrystalOSD/Resources"));
    if folder.join("TEXIMAGE").is_file() && folder.join("hddosd.elf").is_file() {
        Some(folder)
    } else {
        eprintln!("skipped: no resource files (set CRYSTALOSD_RESOURCES)");
        None
    }
}

fn run(args: &[&str], resources: &Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_crystalosd"))
        .current_dir(root())
        .args(args)
        .arg("--resources")
        .arg(resources)
        .output()
        .expect("the crystalosd binary runs");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn no_device(stderr: &str) -> bool {
    ["fatal: instance:", "fatal: physical device:", "fatal: surface:", "SDL: ", "window: "].iter().any(|m| stderr.contains(m))
}

fn gate(baseline: &str, output: &str) {
    let Some(resources) = resources() else { return };
    let (code, stdout, stderr) = run(&["--golden", "check", baseline, "--golden-output", output, "--no-validation", "--mute"], &resources);
    if code != 0 && no_device(&stderr) {
        eprintln!("skipped: no Vulkan device or window: {stderr}");
        return;
    }
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&"golden: 1719 frames identical"), "{stdout}");
    assert!(lines.contains(&"screens visited: main menu, version, main menu, opening System Configuration, System Configuration, inside an entry, System Configuration, hiding the menu, clock alone, showing the menu, System Configuration, closing System Configuration, main menu"), "{stdout}");
    assert!(lines.contains(&"audio: 1719 frames, 1376575 samples, peak 16036, 0 clipped, queue 0..0 frames, 0 underruns, 0 overruns, device none"), "{stdout}");
}

#[test]
fn golden_native() {
    gate("golden/cpp-native.golden", "native");
}

#[test]
fn golden_x2_msaa4() {
    gate("golden/cpp-x2-msaa4.golden", "x2-msaa4");
}

#[test]
fn missing_assets() {
    let scratch = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("missing-assets/resources");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let (code, _, stderr) = run(&[], &scratch);
    assert_eq!(code, 1, "{stderr}");
    for needle in ["missing:", "--bios rom.bin", "--resources dir", &scratch.display().to_string(), "or put those files in"] {
        assert!(stderr.contains(needle), "stderr lacks '{needle}':\n{stderr}");
    }
}

#[test]
fn usage_on_an_unknown_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_crystalosd")).arg("--frobnicate").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage: CrystalOSD [--resources dir] [--bios rom.bin] [--smoke] [--soak seconds]"));
}
