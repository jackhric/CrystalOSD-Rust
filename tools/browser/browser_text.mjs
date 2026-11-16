// The Browser's text callers (HDD OSD 1.10U), as the font calls each makes: the strings it measures (func_00213EE8, at a size)
// and the strings it draws (Font_PutsPackets 0x00213BA8, with the size, the pen and the colour it has set). The font library itself
// (the glyphs, the widths) is facts/text.md's, verified by verify_text2.mjs; here only the callers.
//
// A caller is a generator: it yields { measure: bytes, ratio } and is given back the width; it yields { put: bytes, ratio, x, y,
// rgba } for each string it draws. The constants are read from the program image (`image.word`, `image.single`).
//
//   func_00214200 (x0, x1)            the text area: own+0x14 = x0, own+0x18 = x1 - x0
//   func_0024D3E0 (i, s)              shrink to fit: Font_SetRatio(T[i]), T[i+1] ... while width(s) >= the area's width (T 0x002BD140)
//   func_0024D468 (i, s, t)           the same for two strings: both must be narrower
//   func_00213F88 / 0x0024D870 (y, s) left: pen (x0, y)
//   func_00213FF8 / 0x0024D8A0 (y, s) right: pen (x0 + w - width(s), y)
//   func_00214100 / 0x0024D900 (dx, y, s) centred: pen (x0 + trunc(w / 2) - trunc(width(s) / 2) + dx, y)
//   func_0024D810 (x, y)              Font_SetLocate(x, y)
//   func_002128C8 (i, a)              Font_SetColor(the colour table 0x00348B20 [i], a)
//   func_002433C8 (d, s, limit, n)    the string cut to `limit` with "…" (U+2026), at most n - 1 characters
// On PAL the y of 0x0024D810 / 0x0024D870 / 0x0024D8A0 / 0x0024D900 is (int)(y x 1.15) (D_0036FD98, FDA0, FDA4, FDAC); this
// model is NTSC (the PAL flag D_003706B0 is 0 in every capture): a PAL frame is refused.

export const RATIOS = 0x002bd140;
export const COLOURS = 0x00348b20;
export const GUIDE_X = 0x002bd2e0;
export const TITLE_RATIO = 0x0036fe08;
export const FREE_RATIO = 0x0036fe14;
export const GUIDE_RATIO = 0x0036fe2c;
export const ELLIPSIS = Buffer.from('…', 'utf8');

const half = (v) => Math.trunc(v / 2);
export const ratios = (image) => Array.from({ length: 8 }, (_, i) => image.single(RATIOS + 4 * i));
export const colourOf = (image, index, alpha) => [0, 4, 8].map((k) => image.word(COLOURS + 12 * index + k)).concat([alpha]);

// The title alpha (facts/browser-chrome.md "Titles", 0x00250E20..0x00250E70, NTSC).
export const titleAlpha = (c) => (c < 0xb ? 0 : Math.trunc(((c - 0xa) * 0x80 - (c - 0xa)) / (0x1a - 0xa)));
// The status line's fade (facts/browser-devices.md section 4): the Format dialog's and the status line's alpha.
export const statusAlpha = (empty, pageIn) => Math.trunc((Math.trunc((100 * empty) / 60) * (pageIn - 10)) / (0x1a - 10));

// UTF-8 characters of a string, as sceCccDecodeUTF8 gives them (bytes per character).
function characters(bytes) {
  const out = [];
  for (let i = 0; i < bytes.length;) {
    const b = bytes[i];
    const n = b < 0x80 ? 1 : (b >> 5) === 6 ? 2 : (b >> 4) === 14 ? 3 : 4;
    out.push(bytes.subarray(i, i + n));
    i += n;
  }
  return out;
}

// func_002433C8 (dst, src, limit, n): "…" is measured; the characters are copied one by one; when the skip count is 0 the copy so
// far is measured: no wider than limit - width("…") gives a skip of (that room - width) >> 5 characters; wider than `limit` puts
// "…" after the last measured place and ends; in between, the next character is measured again. n - 1 characters at most.
export function* truncate(src, limit, n, ratio) {
  if (src.length === 0 || n < 5) return src;
  const ellipsis = yield { measure: ELLIPSIS, ratio };
  const room = limit - ellipsis;
  if (room < 0) return src;
  let left = n - 1;
  let skip = 0;
  let saved = 0;
  let out = [];
  const chars = characters(src);
  let k = 0;
  let first = true;
  for (;;) {
    if (!first) left -= 1;
    first = false;
    if (k >= chars.length) return Buffer.concat(out);
    out.push(chars[k++]);
    if (left === 0) return Buffer.concat([...out.slice(0, saved), ELLIPSIS]);
    if (skip !== 0) { skip -= 1; saved = out.length; continue; }
    const width = yield { measure: Buffer.concat(out), ratio };
    if (width <= room) { skip = (room - width) >>> 5; saved = out.length; continue; }
    if (width > limit) return Buffer.concat([...out.slice(0, saved), ELLIPSIS]);
  }
}

// func_0024D3E0 / func_0024D468: the first size from T[start] on at which every string is narrower than the area (the last one
// tried when none is).
export function* shrink(table, start, strings, area) {
  let i = start;
  for (; i < 8; i++) {
    let fits = true;
    for (const s of strings) {
      const width = yield { measure: s, ratio: table[i] };
      if (!(width < area.w)) { fits = false; break; }
    }
    if (fits) return table[i];
  }
  return table[7];
}

// browser_cdplayer_player 0x00250DA0, view mode 1, the top left title (0x00250F90..0x00251034) and, on a card page, the free line
// (0x002512D4..0x0025136C). s1 = 0x38 when the device word D_00370740 is not 0, else 0; s2 = -14 when the aspect word D_00370668
// is 1, else 0; colour 1 when D_003706D0 is 2, else 0, at the title alpha of the page fade-in.
export function* titleLeft(image, s, record, free) {
  const table = ratios(image);
  const s1 = s.device !== 0 ? 0x38 : 0;
  const s2 = s.aspect === 1 ? -14 : 0;
  const rgba = colourOf(image, s.mode === 2 ? 1 : 0, titleAlpha(s.pageIn));
  const area = { x0: s1 + 0x18, w: half(s.width) - 4 - (s1 + 0x18) };
  const text = yield* truncate(record, half(s.width) - (s1 + 0x1c), 0x30, image.single(TITLE_RATIO));
  const ratio = yield* shrink(table, 3, [text], area);
  yield { put: text, ratio, x: area.x0, y: s2 + 0x1f, rgba };
  if (free !== null) yield { put: free, ratio: image.single(FREE_RATIO), x: s1 + 0x18, y: s2 + 0x2b, rgba };
}

// func_00257370 via func_0024D740, the top right title: colour 1 (its alpha is not modelled: `alpha` is the console's), the area
// (trunc(W / 2) + 8, W - 0x18), both lines shrunk together from T[2], each right-aligned, at s3 + 0x1F and s3 + 0x2D (s3 = -14
// when the aspect word is 1, else 0); then Font_SetRatio(1.0).
export function* titleRight(image, s, first, second, alpha) {
  const table = ratios(image);
  const s3 = s.aspect === 1 ? -14 : 0;
  const area = { x0: half(s.width) + 8, w: s.width - 0x18 - (half(s.width) + 8) };
  const rgba = colourOf(image, 1, alpha);
  const ratio = yield* shrink(table, 2, [first, second], area);
  for (const [text, y] of [[first, s3 + 0x1f], [second, s3 + 0x2d]]) {
    const width = yield { measure: text, ratio };
    yield { put: text, ratio, x: area.x0 + area.w - width, y, rgba };
  }
}

// A centred string (func_00214100) in the default area (0x18, W - 0x18) that func_0024D378 restores after the titles.
function* centred(s, text, ratio, y, rgba) {
  const area = { x0: 0x18, w: s.width - 0x30 };
  const width = yield { measure: text, ratio };
  yield { put: text, ratio, x: area.x0 + half(area.w) - half(width), y, rgba };
}

// func_0025BD18, the HDD error dialog: Font_SetRatio(1.0), colour 0 at trunc((n << 7) / 0x1E), the lines up to the first empty
// one centred at 16 i + 0x68 - 8 (count - 1).
export function* dialogLines(image, s, lines, n) {
  const rgba = colourOf(image, 0, Math.trunc((n << 7) / 0x1e));
  const shown = [];
  for (const l of lines) { if (l.length === 0) break; shown.push(l); }
  for (let i = 0; i < shown.length; i++) yield* centred(s, shown[i], 1, 16 * i + 0x68 - 8 * (shown.length - 1), rgba);
}

// browser_button_guide_render 0x002524D0: Font_SetRatio(0.8); per slot s4 with a nonzero counter, colour 0 at min(counter * 8,
// 0x80), the label at the picture's x + 0x1C, y 0xC9 (0xB7 when the aspect word is 2); the fourth slot's label is measured first
// and its picture x is W - width - 0x34. The picture index s2 is s4 with 1 and 2 swapped when the video mode is not 0.
export function* guideLabels(image, s, slots, vidmode) {
  const ratio = image.single(GUIDE_RATIO);
  const y = s.aspect === 2 ? 0xb7 : 0xc9;
  for (const { s4, counter, label } of slots) {
    const s2 = vidmode && s4 === 1 ? 2 : vidmode && s4 === 2 ? 1 : s4;
    let x;
    if (s4 === 3) { const width = yield { measure: label, ratio }; x = s.width - width - 0x34; }
    else x = image.word(GUIDE_X + 16 * s.language + 4 * s2) | 0;
    yield { put: label, ratio, x: x + 0x1c, y, rgba: colourOf(image, 0, Math.min(counter * 8, 0x80)) };
  }
}

// func_00255868, the Format dialog, two lines and (states 1 and 2) the Yes / No line, at the heights of
// model/browser_format.mjs formatDraw: func_0024D550(0, h0, first, h1, second) shrinks both from T[0] and centres each; the Yes / No
// line is at Font_SetRatio(1.0), centred at h2; colour 0 at the status line's fade (0x002516F0 passes it). A one-line state (4, 5)
// is not reached by a capture and is refused.
export function* formatLines(image, s, lines, heights, alpha) {
  if (lines.length < 2) throw new Error('a one-line Format dialog state is not modelled');
  const table = ratios(image);
  const rgba = colourOf(image, 0, alpha);
  const area = { x0: 0x18, w: s.width - 0x30 };
  const ratio = yield* shrink(table, 0, [lines[0], lines[1]], area);
  yield* centred(s, lines[0], ratio, heights[0], rgba);
  yield* centred(s, lines[1], ratio, heights[1], rgba);
  if (lines.length > 2) yield* centred(s, lines[2], 1, heights[2], rgba);
}

// 0x002516F0 (call 0x00251890): the status line, colour 0 at its fade, centred at 0x68 at the size the guide left (1.0).
export function* statusLine(image, s, text, alpha) {
  yield* centred(s, text, 1, 0x68, colourOf(image, 0, alpha));
}
