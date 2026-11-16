# The Browser's device row icons (draft)

Build: **HDD OSD 1.10U**. *Verified* = recomputed by `tools/browser/verify_browser_icons.mjs` (models
`tools/browser/browser_icons_draw.mjs`, `tools/browser/browser_icons_matrix.mjs`, the lab's `References/model/browser_icons.mjs`
for the vertex packets) and equal write for write. *Read* = disassembly. The load of an icon (ICOIMAGE to the vertex and texture
packets) is `References/scripts/verify_ico_decode.mjs`'s.
Run: `CLOCK_BUILD=hdd node tools/browser/verify_browser_icons.mjs <trace.jsonl>`.

| Capture | Cards | Icon draws | Verdict |
|---|---|---|---|
| `hddosd-110U-browser-icons-none` | none | 190 (FHDD) | FOUND 190 icon draws, every write equal; 190 local matrices, 190 matrices A |
| `hddosd-110U-browser-icons-one` | slot 1 blank | 498 (FHDD 249, PS2M 249) | FOUND 498, every write equal; 498 local, 498 A |
| `hddosd-110U-browser-icons-two` | two blank | 1321 (FHDD 377, PS2M 944) | FOUND 1321, every write equal; 1140 local, 1131 A |

The two-card capture moves the cursor over the three items, enters a card page (the Format dialog), leaves it and leaves the
Browser. Controls (a rule bent must fail): the header float + 50 (14 problems), the scale x 1.001 (14), the matrix product in the
other order (14).

## 1. The draw (func_002567D0, *read*)

`func_002567D0(slot, A, B, tick, offset)`: the bounding-box cull (8 corners through B x A, `sceVu0RotTransPersN`),
`func_0023A708(0x3A98)` (TEX0 / BITBLTBUF for the 128 x 128 PSMCT16 texture at DBP 0x3A98), `func_0023A478(B)` (B to
`0x002A7F38`, sent by the chain at `0x002A5A40`: STCYCL, UNPACK 4 qwords to VU1 address 0), `func_0023A898(slot, A)` (A to the
slot block `0x002A80B0 + 0xE0 x slot` + 8), `func_0023ACC0` (the shape weights into the block + 0x50), `func_0023A7F8(slot)`: TADR
= the slot's descriptor (`0x002A5A80 + 0x50 x slot`), CHCR `0x145` at `0x0023A878`.

The descriptor: REF the texture packet (PATH2 DIRECTHL), REF the microprogram for the shape count (`D_002BA120`, copied from
the static descriptors at `0x002A5A80 + 0x50 k`: S = 1 `0x002A6B10` 0x3C qwords, 2 `0x002A6ED0` 0x44, 4 `0x002A7310` 0x45, 6
`0x002A7760` 0x3E, 8 `0x002A7B40` 0x3E), REF the slot block (0xE qwords: STCYCL, UNPACK A to address 4; STCYCL, UNPACK 8 qwords to
address 0x14 = the weights at 20, the header float at 24; MSCAL 0, BASE 0x1C, OFFSET 0x1F2), REF the vertex packet, END.

The lights: `func_0023A580(vectors, colours)` writes three light-vector rows (row 3 = (0, 0, 0, 1)) to `0x002A7FD0` and the colour
matrix to `0x002A7F88` and sends `0x002A5A60` (VU1 addresses 8 and 12). The device row sets them every frame
(`func_00256450(D_00370740, D_00370744)` from `func_002599C8`, `sceVu0NormalLightMatrix` / `sceVu0LightColorMatrix` on the
tables `0x002BD7B0`, `0x002BD810`, `0x002BD870`); kinds 1 and 0xC set their own in their draw (`func_00256520`). One light set in
every capture (*measured*): vectors (-0.577, 0, 0.577), (-0.577, 0.371, 0.577), (-0.577, 0.928, -0.577); colours (0.48, 0.48,
0.43, 1), (0.26, 0.33, 0.5, 1), (0.14, 0.14, 0.38, 1), (0.24, 0.24, 0.24, 1).

## 2. The microprogram, one shape (`0x002A6B10`, 119 instructions)

Every device-row member has one shape and one key of value 1 (*measured*: DISC ... FFIL parsed). The program, *verified* by
running its own instructions (the interpreter in `browser_icons_draw.mjs` reads them from the program image):

- Prologue (MSCAL 0, instructions 0..21 and 91..117): qwords 16..19 = each row of A through B; vf31 = qword 24 (the header float in
  x).
- Per chunk (MSCNT; instructions 22..90): XTOP; the GIF tag copied to the output (input + 277); per vertex, software-pipelined:
  clip = M (x, y, z, 1) (M rows 16..19); factors = max(Lvec (n, 1), 0) (all four fields); c = Lcol factors; Q = 1 / clip.w (DIV,
  ready seven cycles later); ST = (u Q, v Q, Q); RGBA = FTOI0(min(rgba x c, 255)); XYZ = FTOI4(clip x Q).
- The cull: the z of (s_i - s_i-2) x (s_i-1 - s_i-2) (OPMULA / OPMSUB on the screen positions), less the header float, times 1
  (instruction 74); FMAND with 0x20 (the MAC flag Sz) four cycles later; set: ISW 0x8000 into the vertex's XYZ w (ADC, no drawing
  kick).
- XGKICK per chunk; chunks of 3 floor(floor(0x1F0 / (S + 6)) / 3) vertices (69 for one shape).

Arithmetic: the opening's VU1 rules (`opening-lib.mjs` f, add, sub, quotient).

## 3. The item's matrix (func_00259240, *verified*)

Local = `sceVu0RotMatrix`(unit, wrap(item + 0), wrap(item + 4), wrap(item + 8)), `sceVu0TransMatrix`(item + 0x10), rows 0..2 x
(item + 0x60 x `func_00264498()`); A = `sceVu0MulMatrix`(row + 0x30, local) (each row of the local matrix through the row's).
`func_0024DFC8` wraps an angle: beyond `D_0036FDC0` (or below -`D_0036FDC4`) whole periods of `D_0036FDC8` are taken off (the
quotient truncated), then above `D_0036FDCC` it loses `D_0036FDD0`, below `D_0036FDD4` it gains `D_0036FDD8`.

The icon kind (item + 0x130, the icon index: 13 FHDD, 3 PS2M in the captures) picks branches no capture reaches (*read*): DISC (0)
turns by `D_0036FF04` (pi / 8) a draw; CDDA (1) and QUES (0xC) drop y by `D_0036FEFC` (6.4), multiply item + 0x60 by `D_0036FF00`
(1.1) and set their own lights. `func_00264498` gives (0x80 - [0x004E70FC]) / 128 when `func_00263A78()`, else 1.0; 1.0 in every
capture.

B (`D_002BB360`): (1024, 0, 0, 0), (0, 481.28, 0, 0), (2048, 2048, -255.0039, 1), (0, 0, 16777470, 0) on the root page and
(1024, 0, 0, 0), (0, 481.28, 0, 0), (1777, 1967, 144.9961, 1), ... for the card page's header icon (`func_00259540`) (*measured*).

## 4. The Rust port, checked vertex for vertex

`crates/assets/src/ico.rs` decodes ICOIMAGE (the members, the parse with the program's checks, RLE16). `crates/scene/src/browser/
icons.rs` ports section 2 per vertex (`IconVu1<A: Arithmetic>`, the pipeline registers vf26, vf29, vf30 carried from chunk to chunk
and draw to draw) and section 3 (`local_matrix`, `item_matrix`). `crates/scene/tests/console_icons.rs` runs them on the probed
inputs: 2001 icon draws, 249999 vertices equal to the console's card-row passes (x, y on the field's half line, z, RGBA, S, T, Q);
1828 local matrices and 1819 matrices A bit for bit.

Shared with save icons (next task): the decode, the vertex packets, the one-shape program; the morph programs (S = 2, 4, 6, 8) and
`func_0023ACC0`'s weights are not ported (no device-row member has more than one shape).

## Open

- The ADC bit of vertices that close no triangle differs from the model in 386, 3727 and 13366 cases (none, one, two cards): in
  the first loop pass after a program start FMAND does not see instruction 74's flag (the flag the previous activation ended with
  predicts it in 190 of 190, 190 of 190 and 182 of 190 chunk starts), and exact-zero crosses. The captures are PCSX2's VU1; the
  GS draws nothing differently (a triangle list kicks on its third vertex only).
- The item's fields over time: `func_002599C8` per frame (the rocking terms 0.25 sin(2 phase), 0.25 cos(phase), the phase step
  `D_0036FF14` 0.013), `func_00257860` (the grid place: x = -row.F0 + 15 + 24 column, y = -row.F4 + 14 + 24 line, the selection
  scale, the page change), `func_00257FD0` (entry), `func_00259820`, the row matrix (row + 0x30) over a page change, the draw order
  (`D_004DB4F0`). Read in part only.
- The flare `func_00250B00` (two sprites of TEXBNAV1 / TEXBNAV2 at the selected item `D_003707C8`), B's setup, the light tables'
  path through `sceVu0NormalLightMatrix`.
- The bounding-box cull of `func_002567D0` (no capture culls an icon).
