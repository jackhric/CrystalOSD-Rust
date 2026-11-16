// Writes the Browser's TEXB sheets as decoded from a resource folder, raw RGBA, to compare them with the GS memory of a capture.
//   cargo run -p assets --example browser_sheets -- <resource folder> <out dir>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sheets = assets::resources::decode_browser_sheets(std::path::Path::new(&args[1])).expect("the resource folder");
    for info in &assets::clock_textures::BROWSER_TEXTURES {
        let sheet = sheets.iter().find(|s| s.name == info.name).expect("a decoded sheet");
        std::fs::write(std::path::Path::new(&args[2]).join(format!("{}.rgba", info.name)), &sheet.data).expect("written");
        println!("{} {} x {} tbp 0x{:04x}", info.name, sheet.width, sheet.height, info.tbp);
    }
}
