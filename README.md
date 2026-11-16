# CrystalOSD-Rust

A Rust port of [CrystalOSD](https://github.com/JeanxPereira/CrystalOSD), Jean Pereira's reimplementation
of the PlayStation 2 system menu (OSDSYS) as a native desktop application: the boot intro, the main menu,
System Configuration, the Version page, the Crystal Clock and their sound. It runs on Vulkan and SDL3 at
59.94 Hz. Nothing is emulated at run time.

The port started from CrystalOSD at commit `039cc58` and is held to it frame by frame. All credit for the
reverse engineering, the measured specification and the original implementation goes to the upstream
project.

## Status

| Part | State |
|---|---|
| Boot intro, main menu, System Configuration, Version page, clock, sound | Ported; identical to the C++ on the golden gate |
| Browser: shell, page chrome, device logic, text | Written from upstream's measured pages and checked against the console |
| Browser: 3D device row | Drawing is verified; the models' motion is not measured yet, so the app does not show them |
| Browser: save list, save icons, Copy, Delete | Not started |
| `--text sharp` | Optional. Draws text from outlines traced from the console's font |

## Build and run

Rust 1.99 or later, a Vulkan 1.4 driver, `glslc` (shaderc) and SDL3.

```sh
cargo build --release -p app
./target/release/crystalosd
```

- `--no-validation` is needed when the Vulkan validation layers are not installed.
- `--skip-boot` starts at the main menu. `--towers demo` shows the towers in the boot intro.
- `--memory-card-1 card.ps2` and `--memory-card-2 card.ps2` put a memory card image in a slot (read-only).
- Keys: arrows; Enter or Z for ✕; Esc or X for ○; S or Backspace for □; T for △. Gamepads work through SDL.

The app reads the console's resource files from `Resources/`, or from the folder given by `--resources`.
`--bios rom.bin` extracts them from a BIOS image instead.

## How it is checked

- **The golden gate.** `crystalosd --golden check golden/cpp-native.golden` replays a scripted session
  and compares a hash of every frame's scene, pixels and sound with a recording made by the C++ build.
  The pixel hashes are specific to the machine that recorded them (an NVIDIA RTX 2080 SUPER on Linux),
  so on other hardware compare the scene and sound columns.
- **Differential tests.** `cargo test --workspace`. The expected values come from probes compiled from
  the C++ sources; those probes are kept in `tools/cpp_probes/`.
- **Console comparison.** The Browser tests compare the scene with what the console itself sends to its
  graphics chip, vertex for vertex. They read captures taken with upstream's instrumented emulator,
  [Watson](https://github.com/JeanxPereira/Watson), from a data folder outside this repository, and skip
  when it is absent.

## Layout

| Crate | Role |
|---|---|
| `assets` | Decodes the console's resource files, memory card images and icon models |
| `scene` | The logic: one `Frame` of draw passes per tick. No graphics API |
| `audio` | The console's sound queue, sound driver and SPU2, emulated |
| `render` | Vulkan renderer (`ash`) that draws a `Frame` at any resolution |
| `glyph_enhance` | Traces the font's glyphs into outlines for `--text sharp` |
| `app` | The `crystalosd` binary: window, input, settings, golden gate |

## Sony's data

`Resources/` holds the console's own resource files, as the upstream repository does. They are Sony's and
are not covered by this repository's licence.

## Licence

GPL-3.0, as upstream. See `LICENSE`.
