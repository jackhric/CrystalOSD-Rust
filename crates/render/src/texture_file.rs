use std::fs::File;
use std::io::BufReader;
use std::path::Path;

// stbi_load(path, &w, &h, &channels, 4) for PNG: 8 bits per channel (16-bit samples keep their high byte), palette and
// tRNS expanded, grey replicated to RGB, alpha 255 where the file has none.
pub fn load_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path).ok()?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let (w, h) = (info.width, info.height);
    let pixels = w as usize * h as usize;
    let data = &buffer[..info.buffer_size()];
    let line = info.line_size;
    let mut out = Vec::with_capacity(pixels * 4);
    for y in 0..h as usize {
        let row = &data[y * line..];
        for x in 0..w as usize {
            match info.color_type {
                png::ColorType::Rgba => out.extend_from_slice(&row[x * 4..x * 4 + 4]),
                png::ColorType::Rgb => out.extend_from_slice(&[row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 255]),
                png::ColorType::GrayscaleAlpha => out.extend_from_slice(&[row[x * 2], row[x * 2], row[x * 2], row[x * 2 + 1]]),
                png::ColorType::Grayscale => out.extend_from_slice(&[row[x], row[x], row[x], 255]),
                png::ColorType::Indexed => return None,
            }
        }
    }
    Some((w, h, out))
}
