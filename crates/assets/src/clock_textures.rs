use crate::bytes::{Result, error};

// facts/clock-textures.md: resources 0x2B..0x34, the TEXC* members of TEXIMAGE, and the form func_002344F8
// (HDD OSD 1.10U) turns each into 32-bit pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelForm {
    Grey,
    RgbQuarter,
    Alpha,
    GreyAlpha,
    Rgba,
    BlackAlpha,
    Rgb555,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTextureInfo {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub form: PixelForm,
    pub tbp: u32,
}

const fn clock(name: &'static str, width: u32, height: u32, form: PixelForm, tbp: u32) -> ClockTextureInfo {
    ClockTextureInfo { name, width, height, form, tbp }
}

pub const CLOCK_TEXTURES: [ClockTextureInfo; 10] = [
    clock("TEXCFLOW", 64, 64, PixelForm::Grey, 0x2bc0),
    clock("TEXCKABE", 128, 128, PixelForm::RgbQuarter, 0x2c00),
    clock("TEXCBUMP", 64, 64, PixelForm::Grey, 0x2d00),
    clock("TEXCBINV", 64, 64, PixelForm::Grey, 0x2d40),
    clock("TEXCSMOK", 64, 64, PixelForm::Alpha, 0x2d80),
    clock("TEXCREFA", 64, 64, PixelForm::Grey, 0x2dc0),
    clock("TEXCNAVI", 64, 64, PixelForm::Alpha, 0x2e00),
    clock("TEXCBLUR", 64, 64, PixelForm::Alpha, 0x2e40),
    clock("TEXCSTSL", 64, 64, PixelForm::GreyAlpha, 0x2e80),
    clock("TEXCMARU", 64, 64, PixelForm::Rgba, 0x2ec0),
];

/// The GS word address the clock's texture allocator reaches once the ten textures are placed: the end of the last one (a block is 64 words).
pub const fn clock_textures_end() -> u32 {
    const WORDS_PER_BLOCK: u32 = 64;
    const BYTES_PER_BLOCK: u32 = 256;
    let last = &CLOCK_TEXTURES[CLOCK_TEXTURES.len() - 1];
    (last.tbp + last.width * last.height * 4 / BYTES_PER_BLOCK) * WORDS_PER_BLOCK
}

// The opening's textures (facts/opening.md section 2): the OSD's resources 35, 29, 34, 30, 26, 36, 31, 27, 32 are the
// TEXO* members of TEXIMAGE; index is the opening's own texture index. BlackAlpha: (0, 0, 0, a). Rgb555: a 16-byte header,
// then 16-bit texels expanded as the GS reads PSMCT16 with TEXA 127 / 129 and AEM (c << 3; alpha 129 when bit 15, else 127,
// 0 for black), and 8 trailing bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpeningTextureInfo {
    pub name: &'static str,
    pub index: i32,
    pub width: u32,
    pub height: u32,
    pub form: PixelForm,
    pub tbp: u32,
}

const fn opening(name: &'static str, index: i32, width: u32, height: u32, form: PixelForm, tbp: u32) -> OpeningTextureInfo {
    OpeningTextureInfo { name, index, width, height, form, tbp }
}

pub const OPENING_TEXTURES: [OpeningTextureInfo; 9] = [
    opening("TEXOSCE", 0, 256, 64, PixelForm::GreyAlpha, 0x2bc0),
    opening("TEXOFOG1", 2, 64, 64, PixelForm::Rgb555, 0x2cc0),
    opening("TEXOFOG2", 3, 64, 64, PixelForm::Rgb555, 0x2ce0),
    opening("TEXOFOG4", 5, 64, 64, PixelForm::Rgb555, 0x2d20),
    opening("TEXOWAL0", 6, 256, 256, PixelForm::Rgb555, 0x2d40),
    opening("TEXOCRBL", 8, 64, 64, PixelForm::Rgba, 0x3020),
    opening("TEXOREF", 10, 128, 128, PixelForm::Rgb555, 0x3060),
    opening("TEXOBLP", 11, 64, 64, PixelForm::BlackAlpha, 0x30e0),
    opening("TEXOBLPR", 12, 64, 64, PixelForm::BlackAlpha, 0x3120),
];

/// The bytes the form reads: width x height x (1, 2 or 4), or a quarter at 3 bytes for RgbQuarter.
pub fn raw_texture_size(form: PixelForm, width: u32, height: u32) -> usize {
    let pixels = width as usize * height as usize;
    match form {
        PixelForm::RgbQuarter => pixels / 4 * 3,
        PixelForm::Rgb555 => pixels * 2 + 24,
        PixelForm::GreyAlpha => pixels * 2,
        PixelForm::Rgba => pixels * 4,
        PixelForm::Grey | PixelForm::Alpha | PixelForm::BlackAlpha => pixels,
    }
}

// facts/clock-textures.md, func_002344F8 (HDD OSD 1.10U): grey to (g, g, g, 127); alpha to (255, 255, 255, a); grey and
// alpha to (g, g, g, a); RGBA as stored; the 3-byte quarter of TEXCKABE tiled 2 x 2 with alpha 127.
/// R, G, B, A bytes in memory order, width x height.
pub fn convert_texture(raw: &[u8], width: u32, height: u32, form: PixelForm) -> Result<Vec<u8>> {
    let width = width as usize;
    let pixels = width * height as usize;
    if raw.len() < raw_texture_size(form, width as u32, height) {
        return error("texture: fewer bytes than its form needs");
    }
    let mut out = vec![0u8; pixels * 4];
    if form == PixelForm::RgbQuarter {
        let (w, h) = (width / 2, height as usize / 2);
        for y in 0..h {
            for x in 0..w {
                let s = &raw[(y * w + x) * 3..][..3];
                for (dx, dy) in [(0, 0), (w, 0), (0, h), (w, h)] {
                    out[((y + dy) * width + x + dx) * 4..][..4].copy_from_slice(&[s[0], s[1], s[2], 0x7f]);
                }
            }
        }
        return Ok(out);
    }
    for (i, pixel) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let rgba = match form {
            PixelForm::Rgb555 => {
                let v = u32::from(raw[16 + i * 2]) | u32::from(raw[17 + i * 2]) << 8;
                let alpha = if v & 0x7fff == 0 {
                    0
                } else if v & 0x8000 != 0 {
                    129
                } else {
                    127
                };
                [((v & 31) << 3) as u8, (((v >> 5) & 31) << 3) as u8, (((v >> 10) & 31) << 3) as u8, alpha]
            }
            PixelForm::Grey => [raw[i], raw[i], raw[i], 0x7f],
            PixelForm::BlackAlpha => [0, 0, 0, raw[i]],
            PixelForm::Alpha => [0xff, 0xff, 0xff, raw[i]],
            PixelForm::GreyAlpha => [raw[i * 2], raw[i * 2], raw[i * 2], raw[i * 2 + 1]],
            PixelForm::Rgba | PixelForm::RgbQuarter => [raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2], raw[i * 4 + 3]],
        };
        pixel.copy_from_slice(&rgba);
    }
    Ok(out)
}

// facts/browser-chrome.md section 3: browser_texture_indices (0x002BD160) maps the Browser's sheets 0..7 to resources 53..60, the
// TEXB* members of TEXIMAGE; each expands to a 0x14-byte header (type word 4) and CT32 texels, 64 wide (TEXBCDPB 64 x 128), kept
// as stored (alpha 0x80 is 1). tbp: the allocation from 0x2800 in order, 64 blocks per 64 x 64 sheet, 128 for TEXBCDPB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowserTextureInfo {
    pub name: &'static str,
    pub index: i32,
    pub width: u32,
    pub height: u32,
    pub tbp: u32,
}

const fn browser(name: &'static str, index: i32, height: u32, tbp: u32) -> BrowserTextureInfo {
    BrowserTextureInfo { name, index, width: 64, height, tbp }
}

pub const BROWSER_TEXTURES: [BrowserTextureInfo; 8] = [
    browser("TEXBNAV1", 0, 64, 0x2800),
    browser("TEXBNAV2", 1, 64, 0x2840),
    browser("TEXBARRW", 2, 64, 0x2880),
    browser("TEXBBTTN", 3, 64, 0x28c0),
    browser("TEXBCPAR", 4, 64, 0x2900),
    browser("TEXBCDPB", 5, 128, 0x2940),
    browser("TEXBOVAL", 6, 64, 0x29c0),
    browser("TEXBICHI", 7, 64, 0x2a00),
];

pub const BROWSER_TEXTURE_HEADER: usize = 0x14;
