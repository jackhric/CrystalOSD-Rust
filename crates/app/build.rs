fn main() {
    println!("cargo:rerun-if-changed=imgui/backends.cpp");
    #[cfg(feature = "debug-panel")]
    imgui_backends();
}

#[cfg(feature = "debug-panel")]
fn imgui_backends() {
    use std::env;
    use std::path::PathBuf;
    use std::process::Command;

    let imgui = PathBuf::from(
        env::var("DEP_DEAR_IMGUI_THIRD_PARTY").or_else(|_| env::var("DEP_DEAR_IMGUI_IMGUI_INCLUDE_PATH")).expect("dear-imgui-sys's include path"),
    );
    let mut build = cc::Build::new();
    build.cpp(true).std("c++17").include(&imgui).file("imgui/backends.cpp");
    for (key, value) in env::vars() {
        if let Some(define) = key.strip_prefix("DEP_DEAR_IMGUI_DEFINE_") {
            build.define(define, value.as_str());
        }
    }
    if let Ok(out) = Command::new("pkg-config").args(["--cflags-only-I", "sdl3"]).output() {
        for flag in String::from_utf8_lossy(&out.stdout).split_whitespace() {
            if let Some(path) = flag.strip_prefix("-I") {
                build.include(path);
            }
        }
    }
    build.compile("crystalosd-imgui-backends");
}
