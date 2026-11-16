// A Browser capture as a fixture for the Rust scene's console comparison (crates/scene/tests/console_browser.rs).
//   node tools/browser/export_browser_fixture.mjs <capture name> [<out dir>] [--pcs 0x213ba8,0x213f14,...]
// --pcs keeps only the probe records at those program counters (a capture with the text verifiers' probes is too large whole).
// Reads <captures>/<name>.trace.jsonl and, when present, <name>.gs; writes <out dir> (default
// $CRYSTALOSD_DATA/fixtures/<name>):
//   probes.json   every probe record of the trace (pc, probe index, capture frame, packet position, registers and the
//                 memory it read, as hex), and the trace's pad and write inputs: the inputs the scene is computed from
//   passes.json   the frames of the GS dump as tools/parity/frame_passes.mjs writes them (same units, same state
//                 description), each pass marked `card` when it belongs to the card row (HDD OSD 0x002527A8..0x002538F0,
//                 ROM 2.30 0x0024C238..0x0024D58C, the rule of verify_browser_chrome.mjs) and `browser` for the frames
//                 the master draw or the card row's root ran in. The draws are tied to the trace's packets through Watson's parser (its
//                 `trace` option), so the card row is cut by the probes, not guessed.
// Needs the lab checkout (CRYSTALOSD_ROOT, default ../CrystalOSD) and Watson's server build (WATSON_DIST).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const LAB = path.resolve(process.env.CRYSTALOSD_ROOT ?? path.join(HERE, '../../../CrystalOSD'));
const homes = await import(path.join(LAB, 'References/lib/data.mjs'));
const { readTrace } = await import(path.join(LAB, 'References/lib/trace.mjs'));
const { describeState, mipLevels, mipOf, mipRegisters, nativeVertices } = await import(path.join(LAB, 'tools/parity/make_fixture.mjs'));
const DIST = (process.env.WATSON_DIST ?? homes.WATSON_DIST).replace(/\\/g, '/').replace(/\/$/, '');
const { parseGsDump } = await import(`file:///${DIST}/gs/parse.js`);
const { decodeRegister } = await import(`file:///${DIST}/gs/registers.js`);

const args = process.argv.slice(2);
const pcsAt = args.indexOf('--pcs');
const keepPcs = pcsAt >= 0 ? new Set(args[pcsAt + 1].split(',').map((x) => parseInt(x, 16))) : null;
if (pcsAt >= 0) args.splice(pcsAt, 2);
const [name, outArg] = args;
if (!name) { console.error('usage: node tools/browser/export_browser_fixture.mjs <capture name> [<out dir>]'); process.exit(2); }
const CAPTURES = process.env.CRYSTALOSD_CAPTURES ?? homes.CAPTURES;
const tracePath = path.join(CAPTURES, `${name}.trace.jsonl`);
const dumpPath = path.join(CAPTURES, `${name}.gs`);
const out = outArg ?? path.join(homes.DATA_HOME, 'fixtures', name);
fs.mkdirSync(out, { recursive: true });

const hex = (bytes) => Buffer.from(bytes).toString('hex');

const trace = readTrace(tracePath);
if (!trace.complete) throw new Error(`${tracePath}: ${trace.reason}`);
// The build: a ROM 2.30 capture's name (rom-0230A-...) or what it probed: ROM 2.30's master draw 0x0024D500, card row
// 0x0024C238 or grid-page input 0x0024E440 (HDD OSD 1.10U has no code at these three).
const ROM_PCS = new Set([0x0024d500, 0x0024c238, 0x0024e440]);
const rom = /rom-0230/.test(name) || trace.probes.some((p) => ROM_PCS.has(p.pc));
const CARD = rom ? [0x0024c238, 0x0024d58c] : [0x002527a8, 0x002538f0];
const MASTER = rom ? 0x0024d500 : 0x00253860;
const probes = trace.probes.filter((p) => !p.preroll && (!keepPcs || keepPcs.has(p.pc))).map((p) => ({
  pc: p.pc, index: p.index ?? null, frame: p.captureFrame, at: p.at,
  gpr: p.gpr.map((g) => Number(BigInt.asUintN(32, BigInt(g)))),
  mem: p.mem.filter((m) => m.bytes).map((m) => ({ address: m.address, hex: hex(m.bytes) })),
}));
const inputs = (trace.inputs ?? []).map((i) => ({ ...i, bytes: i.bytes ? hex(i.bytes) : undefined }));
fs.writeFileSync(path.join(out, 'probes.json'), JSON.stringify({ capture: name, build: rom ? 'rom' : 'hdd', firstFrame: trace.frame, probeSpec: trace.probeSpec ?? null, inputs, probes }));
console.log(`${probes.length} probe records -> ${path.join(out, 'probes.json')}`);

if (fs.existsSync(dumpPath)) {
  const tmp = path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'browser-fixture-')), 'parsed.jsonl');
  parseGsDump(dumpPath, tmp, { trace: tracePath });
  const records = fs.readFileSync(tmp, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
  fs.rmSync(path.dirname(tmp), { recursive: true, force: true });
  const draws = records.filter((r) => r.type === 'draw');

  // The card row's packets: between a probe at its root and the next probe at its return (verify_browser_chrome.mjs).
  const starts = probes.filter((p) => p.pc === CARD[0]), ends = probes.filter((p) => p.pc === CARD[1]);
  const windows = starts.map((start, k) => [start.at, ends.find((e) => e.at >= start.at && (k + 1 === starts.length || e.at <= starts[k + 1].at))?.at ?? Infinity]);
  const inCard = (packet) => windows.some(([lo, hi]) => packet >= lo && packet < hi);
  const masterFrames = new Set(probes.filter((p) => p.pc === MASTER || p.pc === CARD[0]).map((p) => p.frame));

  const KIND = { sprite: ['Sprites', 2], line: ['Lines', 2], linestrip: ['Lines', 2], triangle: ['Triangles', 3], tristrip: ['Triangles', 3], trifan: ['Triangles', 3] };
  const targetOfBlock = new Map();
  for (const d of draws) {
    const block = Number(decodeRegister('FRAME', BigInt(d.state.FRAME)).FBP) * 32;
    targetOfBlock.set(block, `fb${block.toString(16).padStart(4, '0')}`);
  }
  let mipValues = null;
  const levelsOf = (ctxt) => mipLevels(mipValues ??= mipRegisters(fs.readFileSync(dumpPath)), ctxt);
  const frames = [];
  for (const d of draws) {
    const kind = KIND[d.primitive];
    if (!kind) throw new Error(`draw ${d.index}: primitive ${d.primitive}`);
    const [primitive, size] = kind;
    const primitives = [];
    for (let i = 0; i < d.indices.length; i += size) primitives.push(d.indices.slice(i, i + size).map((at) => d.vertices[at]));
    const state = describeState(d.state, targetOfBlock, primitive, mipOf(d.vertices, levelsOf));
    if (state.smooth && primitive === 'Lines') state.skip ||= 'gouraud line';
    let texture = null;
    if (state.texture) { const { block, pages, ...rest } = state.texture; texture = rest; }
    const frame = (frames[d.frame] ??= { frame: d.frame, field: 0, browser: masterFrames.has(d.frame), passes: [] });
    frame.field ||= state.field;
    frame.passes.push({
      index: d.index, name: `draw-${d.index}`, target: targetOfBlock.get(state.block), primitive, field: state.field,
      scissor: state.scissor, blend: state.blend, antialias: state.antialias, depth: state.depth, texture, skip: state.skip,
      card: d.packet >= 0 && inCard(d.packet), packet: d.packet,
      vertices: nativeVertices(primitive, primitives, state),
    });
  }
  for (let k = 0; k < frames.length; k++) frames[k] ??= { frame: k, field: 0, browser: masterFrames.has(k), passes: [] };
  fs.writeFileSync(path.join(out, 'passes.json'), JSON.stringify({ capture: name, frames }));
  const card = frames.reduce((n, f) => n + f.passes.filter((p) => p.card).length, 0);
  console.log(`${frames.length} frames (${frames.filter((f) => f.browser).length} with the master draw), ${draws.length} passes, ${card} of the card row -> ${path.join(out, 'passes.json')}`);
}
