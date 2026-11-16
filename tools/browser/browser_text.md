# The Browser's text (draft)

Builds: **HDD OSD 1.10U** (canon); **ROM 2.30** for the library only (section 4).
*Verified* = recomputed by `tools/browser/verify_browser_text.mjs` (model `tools/browser/browser_text.mjs`) and equal call for
call, in order, frame by frame. *Read* = disassembly (address range given). The font library itself (glyph lookup, widths, the
glyph cache, the 12-vertex fans) is `facts/text.md`'s, verified by `References/scripts/verify_text2.mjs` on the same captures.
Run: `CLOCK_BUILD=hdd node tools/browser/verify_browser_text.mjs <trace.jsonl>`.

| Capture | Build | Browser frames | Verdict |
|---|---|---|---|
| `hddosd-110U-browser-text-session` | HDD OSD | 597 | FOUND 3138 strings and 4927 measurements equal in 597 frames |
| `hddosd-110U-browser-text-session` | HDD OSD | (library) | `verify_text2.mjs` FOUND, 3488 strings, 29111 glyphs |
| `rom-0230A-browser-text-blank` | ROM 2.30 | (library) | `verify_text2.mjs` FOUND, 1754 strings |

Strings drawn, by caller: title left 597, title right 814, error dialog 216, guide 978, free line 190, Format dialog 258, status
line 85. Controls (a rule bent must fail): the screen width + 2 (578 frames fail), the aspect word set to 1 (597), the page fade-in
+ 1 (559), the dialog counter + 1 (72).

## 1. The library calls the callers make

Every caller sets the program's font state (own = `0x003979B0`) and calls `Font_PutsPackets` (`0x00213BA8`) or measures with
`func_00213EE8` (width = (int)((float)(int)reach + pitch x xscale), *verified* through `verify_text2.mjs`).

- `func_00214200(x0, x1)`: the text area, own + 0x14 = x0, own + 0x18 = x1 - x0. `func_0024D3C0(dx)` sets (dx + 0x18, W/2 - 4),
  `func_0024D3A0` (W/2 + 8, W - 0x18), `func_0024D378` (0x18, W - 0x18) (*read*).
- Shrink to fit, `func_0024D3E0(i, s)` / `func_0024D468(i, s, t)`: `Font_SetRatio(T[i])`, `T[i+1]`, ... while a string is not
  narrower than the area; T at `0x002BD140` = [1, 0.875, 0.8, 0.77, 0.73, 0.7, 0.67, 0.61]; the last one when none fits.
- Placement: left `func_00213F88` pen (x0, y); right `func_00213FF8` pen (x0 + w - width, y); centred `func_00214100` pen
  (x0 + trunc(w / 2) - trunc(width / 2) + dx, y).
- Colour `func_002128C8(i, a)`: the table `0x00348B20` (12 bytes a row: 0 grey 96, 96, 96; 1 yellow 110, 110, 0) at alpha a.
- Truncation `func_002433C8(d, s, limit, n)`: "…" (U+2026) is measured; characters are copied one by one; when the skip count is
  0 the copy so far is measured: no wider than limit - width("…") gives a skip of (that room - width) >> 5 characters; wider than
  the limit puts "…" after the last measured place and ends; at most n - 1 characters.
- PAL multiplies y by 1.15 (`D_0036FD98`, `FDA0`, `FDA4`, `FDAC`, *read*); every capture is NTSC and the model refuses PAL.

## 2. The strings

| String | Caller (return address) | Size | Pen | Colour, alpha |
|---|---|---|---|---|
| Title left (record at `0x002BD420` + 0x1C0 x depth + 0x12C) | `0x00251034` | truncated to W/2 - (s1 + 0x1C), 0x30 characters, at `0x0036FE08` (0.61); shrunk from T[3] in the area (s1 + 0x18, W/2 - 4) | (s1 + 0x18, s2 + 0x1F) | 1 when the mode word `D_003706D0` is 2, else 0; the title alpha of the page fade-in |
| Free line ("7,998 KB Free", "Unformatted") | `0x00251374` | `0x0036FE14` (0.6) | (s1 + 0x18, s2 + 0x2B) | as the title |
| Title right, two lines | `0x0024D780`, `0x002575F0` | both shrunk together from T[2] in (W/2 + 8, W - 0x18) | right-aligned at s3 + 0x1F and s3 + 0x2D | 1 |
| HDD error dialog lines | `0x0025C038` | 1.0 | centred in (0x18, W - 0x18), y = 16 i + 0x68 - 8 (n - 1), the lines up to the first empty one | 0 at trunc((n << 7) / 0x1E) |
| Guide labels | `0x00252754` | `0x0036FE2C` (0.8) | x = table `0x002BD2E0`[language x 16 + s2 x 4] + 0x1C; slot 3 ("Options") W - width - 0x34 + 0x1C, measured every frame; y 0xC9 (0xB7 when the aspect word is 2) | 0 at min(counter x 8, 0x80) |
| Format dialog, two lines | `0x0024D594`, `0x002558F4` | shrunk together from T[0] | centred at the heights of `formatDraw` | 0 at the status fade |
| Format dialog Yes / No | `0x00255964` | 1.0 | centred at the third height | the format `0x00366490` ("\x07c%1d%s    \x07c%1d%s") with the colours of `formatDraw` |
| Status line ("No data.", "Unformatted") | `0x00253994` | 1.0 (left by the guide) | centred, y 0x68 | 0 at the status fade |

s1 = 0x38 when the device word `D_00370740` is not 0, else 0; s2 = s3 = -14 when the aspect word `D_00370668` is 1, else 0.

## 3. The glyph cache

`module_browser_257100` calls `func_00214438` = `scePFontSetTexMem(ctx, 0x2A40 << 6, 0x120 << 6, 0x3E80 << 6)` at the thread's
start (`0x0024EAD8`): the context `0x003969B0` is the clock's; its list (codes and order) carries over, and the cache is laid out
again at the first glyph (*verified* by the Rust harness below). Measurements move characters to the head of the list, so the
calls of section 1 decide the cells.

A drawn glyph of a block with another cell size lays the cache out again inside a frame (*measured*, frame 241 of
`hddosd-110U-browser-text-session2`: the mark `0xD81B` of "Memory Card \x07o027/1", block `0x2D1DD0` of FNTOSD, 64 x 40, lays
the cache out with 72-wide cells; the "/" after it lays it out again with 40-wide cells). The glyphs drawn before keep the layout
they were drawn from.

## 4. ROM 2.30

The library is verified (`verify_text2.mjs` FOUND on `rom-0230A-browser-text-blank`); the ROM callers (ROM 2.30 `0x0024D500`
master draw) are not modelled: ROM 2.30 draws its Browser text as sprites (`facts/browser-chrome.md` section 5).

## 5. The Rust port, checked glyph for glyph

`crates/scene/src/browser/strings.rs` ports section 1 and 2; `crates/scene/src/text.rs` gives callers outside the clock the
library calls and keeps one layout per relayout within a frame (`TextFrame::glyph_layouts`). `crates/scene/tests/console_screen.rs`
`screen_with_text` runs the screen from the pad words of `hddosd-110U-browser-text-session2`: 407 frames equal pass for pass with
the text included, and the 190 card-page frames (their background variant is not specified) equal on their text passes alone:
25846 glyph fans equal.

APP POLICY measured there: on a card page the Format dialog drawn, and slot 2 of the guide, are the dialog as it was at the pass's
start (Left at 391, Circle at 451 show in the next pass).

## Open

- The title-left truncation limit and area width are reached by no capture (no title near the limit; mutants of those constants
  survive).
- Non-ASCII UTF-8 lengths in the truncation are reached by no capture.
- PAL; the one-line Format dialog states 4 and 5; ROM 2.30's callers.
