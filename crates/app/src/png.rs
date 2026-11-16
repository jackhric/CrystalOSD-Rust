use std::fs;
use std::path::Path;

fn crc(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, t) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut value = !0u32;
    for &b in data {
        value = table[((value ^ u32::from(b)) & 0xff) as usize] ^ (value >> 8);
    }
    !value
}

fn big(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    big(out, data.len() as u32);
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let c = crc(&out[start..]);
    big(out, c);
}

// An 8-bit RGBA PNG, stored without compression.
pub fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("png: wrong pixel count".to_owned());
    }
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    let mut z = vec![0x78, 0x01];
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + u32::from(v)) % 65521;
        b = (b + a) % 65521;
    }
    let mut at = 0;
    while at < raw.len() {
        let n = (raw.len() - at).min(65535);
        z.push(u8::from(at + n == raw.len()));
        z.push(n as u8);
        z.push((n >> 8) as u8);
        z.push(!n as u8);
        z.push((!n >> 8) as u8);
        z.extend_from_slice(&raw[at..at + n]);
        at += n;
    }
    big(&mut z, (b << 16) | a);

    let mut header = Vec::new();
    big(&mut header, width);
    big(&mut header, height);
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut file = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    chunk(&mut file, b"IHDR", &header);
    chunk(&mut file, b"IDAT", &z);
    chunk(&mut file, b"IEND", &[]);
    Ok(file)
}

pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = png_bytes(width, height, rgba)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("filesystem error: cannot create directories: {e}"))?;
    }
    fs::write(path, file).map_err(|_| format!("png: cannot write {}", path.display()))
}
