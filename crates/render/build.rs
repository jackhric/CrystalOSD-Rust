use std::path::PathBuf;
use std::process::Command;

// The C++ CMakeLists.txt: `glslc -o <bin>/shaders/<name>.spv Shaders/<name>`, no other flag.
fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let sources = manifest.join("../../shaders");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("shaders");
    std::fs::create_dir_all(&out).unwrap();
    let glslc = std::env::var_os("VULKAN_SDK")
        .map(|sdk| PathBuf::from(sdk).join("bin").join(if cfg!(windows) { "glslc.exe" } else { "glslc" }))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("glslc"));
    for name in ["Native.vert", "Native.frag"] {
        let source = sources.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let spv = out.join(format!("{name}.spv"));
        let status = Command::new(&glslc)
            .arg("-o")
            .arg(&spv)
            .arg(&source)
            .status()
            .unwrap_or_else(|e| panic!("glslc not found ({e}): install the Vulkan SDK or set VULKAN_SDK"));
        assert!(status.success(), "glslc failed on {}", source.display());
    }
    println!("cargo:rerun-if-env-changed=VULKAN_SDK");
}
