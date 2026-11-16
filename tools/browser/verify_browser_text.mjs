// The Browser's text, HDD OSD 1.10U NTSC: for every string the Browser draws, the size, the pen and the colour its caller sets, and
// the strings the caller measures on the way, recomputed by browser_text.mjs from the probed words and compared, call for call, with
// what the console did. The font library (the glyphs and the widths) is verified by verify_text2.mjs on the same capture; a width
// here is the console's (the probe at the return of func_00213EE8).
//
//   Font_PutsPackets 0x00213BA8 (entry): the string, the program's font state (own = 0x003979B0: +0x14 / +0x18 the text area,
//     +0x28 the size, +0x30 the pen, +0x40 the colour), the gp words the callers read, the guide counters, the page records
//   func_00213EE8 0x00213F14 (calcDrawArea returned; s0 = the string) and 0x00213F48 (v0 = the width): the string measured, the size
//     and the width
//   0x00252574 (after get_vidmode_with_fallback, v0): the video mode the guide swaps its pictures by
//   0x0024EAD8 (module_browser_24EA80, the call of func_00214438 = scePFontSetTexMem): the library's context and the program's font
//     state (for the Rust harness)
//
// CLOCK_BUILD=hdd node verify_browser_text.mjs <trace.jsonl>
import { readTraceFor, dataPath } from '../../../CrystalOSD/References/lib/index.mjs';
import { range, pc } from '../../../CrystalOSD/References/scripts/builds.mjs';
import { elfImage } from '../../../CrystalOSD/References/model/clock_text.mjs';
import { formatDraw } from '../../../CrystalOSD/References/model/browser_format.mjs';
import fs from 'node:fs';
import { titleLeft, titleRight, dialogLines, guideLabels, formatLines, statusLine, statusAlpha } from './browser_text.mjs';

const OWN = 0x003979b0;
const P = { put: 0x00213ba8, measuring: 0x00213f14, measured: 0x00213f48, vidmode: 0x00252574, texMem: 0x0024ead8 };
export const PROBES = [
  { pc: pc(P.put), ranges: ['a0:0x100', range(OWN, 0x50), range(0x00370660, 0x120), range(0x003708f0, 4), range(0x00370bb8, 0x10), range(0x004da340, 0x10),
    range(0x001f0cb4, 8), range(0x002bd420, 0x380)] },
  { pc: pc(P.measuring), ranges: ['s0:0x100', range(OWN, 0x50)] },
  { pc: pc(P.measured), ranges: [] },
  { pc: pc(P.vidmode), ranges: [] },
  { pc: pc(P.texMem), ranges: [range(0x003969b0, 0x1160)] },
];

// The callers, by the return address Font_PutsPackets sees.
const CALLER = {
  0x00251034: 'title left', 0x00251374: 'free line', 0x0024d780: 'title right', 0x002575f0: 'title right',
  0x0025c038: 'dialog', 0x00252754: 'guide', 0x0024d594: 'format', 0x002558f4: 'format', 0x00255964: 'format', 0x00253994: 'status',
};

const image = (() => { const img = elfImage(fs.readFileSync(dataPath('dumps/hddosd-host/hddosd.elf'))); return { word: img.word, single: (a) => { const b = Buffer.alloc(4); b.writeUInt32LE(img.word(a) >>> 0); return b.readFloatLE(0); } }; })();

const find = (r, a, n) => { for (const m of r.mem) if (m.bytes && a >= m.address && a + n <= m.address + m.bytes.length) return m.bytes.subarray(a - m.address, a - m.address + n); throw new Error(`0x${a.toString(16)} is not in the probe at 0x${r.pc.toString(16)}`); };
const cString = (r, a) => { for (const m of r.mem) if (m.bytes && a >= m.address && a < m.address + m.bytes.length) { const s = m.bytes.subarray(a - m.address); const end = s.indexOf(0); return Buffer.from(s.subarray(0, end < 0 ? s.length : end)); } throw new Error('the string is not in the probe'); };
const s32 = (r, a) => find(r, a, 4).readInt32LE(0);
const f32 = (r, a) => find(r, a, 4).readFloatLE(0);

// One call as the console made it.
function callOf(r) {
  const ownRatio = f32(r, OWN + 0x28);
  if (r.pc === P.measuring) return { kind: 'measure', text: cString(r, r.gpr[16] >>> 0), ratio: ownRatio, width: null, frame: r.captureFrame };
  return {
    kind: 'put', text: cString(r, r.gpr[4] >>> 0), ratio: ownRatio, ra: r.gpr[31] >>> 0, frame: r.captureFrame,
    x: f32(r, OWN + 0x30), y: f32(r, OWN + 0x34), rgba: [0, 4, 8, 12].map((k) => Math.round(f32(r, OWN + 0x40 + k) * 128)),
    area: [s32(r, OWN + 0x14), s32(r, OWN + 0x18)], record: r,
  };
}

// The words the callers read, from a put record of the frame.
function wordsOf(r) {
  return {
    width: s32(r, 0x001f0cb4), aspect: s32(r, 0x00370668), language: s32(r, 0x003706ac), pal: s32(r, 0x003706b0), mode: s32(r, 0x003706d0),
    depth: s32(r, 0x00370728), pageIn: s32(r, 0x0037072c), device: s32(r, 0x00370740), empty: s32(r, 0x00370764), n: s32(r, 0x003708f0),
    formatState: s32(r, 0x00370bc0), formatCursor: find(r, 0x00370bbc, 1)[0], counters: [0, 1, 2, 3].map((k) => s32(r, 0x004da340 + 4 * k)),
  };
}

// The callers of one frame, in the master draw's order, from the strings it drew; `bend` perturbs a rule (the negative control).
function callersOf(calls, vidmode, bend) {
  const puts = calls.filter((c) => c.kind === 'put');
  const by = (name) => puts.filter((c) => CALLER[c.ra] === name);
  const s0 = wordsOf(puts[0].record);
  if (s0.pal !== 0) throw new Error('a PAL frame: the model is NTSC');
  const s = { ...s0, width: s0.width + (bend === 'width' ? 2 : 0), aspect: bend === 'aspect' ? 1 : s0.aspect, pageIn: s0.pageIn + (bend === 'page' ? 1 : 0) };
  const out = [];
  const left = by('title left');
  if (left.length) {
    const record = cString(left[0].record, 0x002bd420 + 0x1c0 * s.depth + 0x12c);
    const free = by('free line')[0];
    out.push(titleLeft(image, s, record, free ? free.text : null));
  }
  const right = by('title right');
  if (right.length) out.push(titleRight(image, s, right[0].text, right[1] ? right[1].text : Buffer.alloc(0), right[0].rgba[3]));
  const dialog = by('dialog');
  if (dialog.length) out.push(dialogLines(image, s, dialog.map((c) => c.text), wordsOf(dialog[0].record).n + (bend === 'dialog' ? 1 : 0)));
  const guide = by('guide');
  if (guide.length) {
    const slots = s.counters.map((counter, s4) => ({ s4, counter })).filter((x) => x.counter !== 0).map((x, k) => ({ ...x, label: guide[k] ? guide[k].text : Buffer.alloc(0) }));
    out.push(guideLabels(image, s, slots, vidmode));
  }
  const format = by('format');
  if (format.length) {
    const w = wordsOf(format[0].record);
    const draw = formatDraw({ state: w.formatState, cursor: w.formatCursor });
    out.push(formatLines(image, s, format.map((c) => c.text), draw.heights, statusAlpha(w.empty, w.pageIn)));
  }
  const status = by('status');
  for (const c of status) { const w = wordsOf(c.record); out.push(statusLine(image, s, c.text, statusAlpha(w.empty, w.pageIn))); }
  return out;
}

const same = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)) === 0;

// One frame: every call of the frame must be the next one the callers make. Returns the problems.
function replay(calls, vidmode, tally, bend) {
  const problems = [];
  let at = 0;
  for (const caller of callersOf(calls, vidmode, bend)) {
    let step = caller.next();
    while (!step.done) {
      const want = step.value;
      const got = calls[at++];
      if (!got) { problems.push(`the console made no more calls; the model wants ${want.put ? 'a put' : 'a measure'} of "${(want.put ?? want.measure).toString('latin1')}"`); return problems; }
      if (want.measure) {
        if (got.kind !== 'measure' || !same(got.text, want.measure) || got.ratio !== want.ratio) {
          problems.push(`frame ${got.frame}: the console ${got.kind}s "${got.text.toString('latin1')}" at ${got.ratio}, the model measures "${want.measure.toString('latin1')}" at ${want.ratio}`);
          return problems;
        }
        tally.measures++;
        step = caller.next(got.width);
      } else {
        const ok = got.kind === 'put' && same(got.text, want.put) && got.ratio === want.ratio && got.x === want.x && got.y === want.y && want.rgba.every((v, k) => v === got.rgba[k]);
        if (!ok) {
          problems.push(`frame ${got.frame}: ${CALLER[got.ra] ?? `ra 0x${got.ra.toString(16)}`} ${got.kind} "${got.text.toString('latin1')}" ratio ${got.ratio} pen (${got.x},${got.y}) rgba ${got.rgba}, the model "${want.put.toString('latin1')}" ${want.ratio} (${want.x},${want.y}) ${want.rgba}`);
          return problems;
        }
        tally.puts++;
        tally.by[CALLER[got.ra]] = (tally.by[CALLER[got.ra]] ?? 0) + 1;
        step = caller.next();
      }
    }
  }
  if (at !== calls.length) problems.push(`frame ${calls[at].frame}: ${calls.length - at} calls the model does not make, the first ${calls[at].kind} "${calls[at].text.toString('latin1')}"${calls[at].ra === undefined ? '' : ` ra 0x${calls[at].ra.toString(16)}`}`);
  return problems;
}

export function verify(file, bend = null) {
  const trace = readTraceFor(file, PROBES);
  if (!trace.complete) throw new Error(`${file}: ${trace.reason}`);
  const frames = new Map();
  const vidmodes = new Map();
  for (const r of trace.probes) {
    if (r.preroll) continue;
    if (r.pc === P.vidmode) vidmodes.set(r.captureFrame, (r.gpr[2] | 0) !== 0);
    if (r.pc === P.measured) {
      const open = [...frames.values()].flat().findLast((c) => c.kind === 'measure' && c.width === null);
      if (!open) throw new Error(`frame ${r.captureFrame}: a width with no string measured`);
      open.width = r.gpr[2] | 0;
      continue;
    }
    if (r.pc !== P.put && r.pc !== P.measuring) continue;
    if (!frames.has(r.captureFrame)) frames.set(r.captureFrame, []);
    frames.get(r.captureFrame).push(callOf(r));
  }
  const tally = { frames: 0, puts: 0, measures: 0, by: {} };
  const problems = [];
  for (const [frame, calls] of frames) {
    if (!calls.some((c) => c.kind === 'put' && CALLER[c.ra])) continue;
    const vidmode = vidmodes.has(frame) ? vidmodes.get(frame) : false;
    const p = replay(calls, vidmode, tally, bend);
    problems.push(...p);
    if (!p.length) tally.frames++;
  }
  return { tally, problems, framesSeen: [...frames.values()].filter((c) => c.some((x) => x.kind === 'put' && CALLER[x.ra])).length };
}

if (process.argv[1] && process.argv[1].endsWith('verify_browser_text.mjs')) {
  const file = process.argv[2];
  const result = verify(file);
  console.log(`Browser frames ${result.framesSeen}, equal ${result.tally.frames}; strings drawn ${result.tally.puts} ${JSON.stringify(result.tally.by)}; measurements ${result.tally.measures}`);
  for (const p of result.problems.slice(0, 12)) console.log(`  ${p}`);
  const controls = ['width', 'aspect', 'page', 'dialog'].map((bend) => [bend, verify(file, bend).problems.length]);
  console.log(`controls (a rule bent must fail): ${controls.map(([b, n]) => `${b} ${n}`).join(', ')}`);
  const ok = result.problems.length === 0 && result.tally.puts > 0 && controls.every(([, n]) => n > 0);
  console.log(`verdict: ${ok ? `FOUND ${result.tally.puts} strings and ${result.tally.measures} measurements equal in ${result.tally.frames} frames` : `PARTIAL ${result.problems.length} frames differ`}`);
  process.exit(ok ? 0 : 3);
}
