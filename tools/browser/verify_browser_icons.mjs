// The Browser's 3D device row, HDD OSD 1.10U NTSC: every icon draw recomputed from the probed inputs and compared, vertex for vertex,
// with the triangles the GS drew.
//
//   func_00259240 0x00259240 (entry): the item (a0, its record) and the row (a1); its matrix and lights
//   0x002593A8 (after func_0024DCE0): the item's local matrix on the stack
//   func_002567D0 0x002567D0 (entry): slot a0, matrix A (a1), matrix B (a2), tick a3, offset t0
//   func_0023A580 0x0023A580 (entry): the light vectors (a0, 3 rows) and the light colour matrix (a1)
//   func_0023A7F8 0x0023A878 (the CHCR write): the static packets (matrix B, the lights) and the slot blocks the chain sends to VU1
//   module_browser_23AFC8 0x0023AFC8 (entry): slot a0 loaded with the member at a1 (the resource rows say which)
//
// The model (browser_icons_draw.mjs): VU1 data memory as the chains leave it, the microprogram of one shape (0x002A6B10, 119
// instructions) run per vertex, the GIF packets it kicks. The member's vertex packet is ../model/browser_icons.mjs's.
//
// CLOCK_BUILD=hdd node verify_browser_icons.mjs <trace.jsonl> [passes.json]
import { range, pc } from '../../../CrystalOSD/References/scripts/builds.mjs';

export const SLOTS = 12;
export const P = {
  item: 0x00259240, local: 0x002593a8, draw: 0x002567d0, lights: 0x0023a580, kick: 0x0023a878, load: 0x0023afc8,
};
export const PROBES = [
  { pc: pc(P.item), ranges: ['a0:0x6a0', 'a1:0x70', range(0x0036fef0, 0x20), range(0x00370960, 4), range(0x003709a0, 4), range(0x003706f8, 4), range(0x00370698, 4)] },
  { pc: pc(P.local), ranges: ['sp:0x60'] },
  { pc: pc(P.draw), ranges: ['a1:0x40', 'a2:0x40'] },
  { pc: pc(P.lights), ranges: ['a0:0x30', 'a1:0x40'] },
  { pc: pc(P.kick), ranges: [range(0x002a7f30, 0xe0), range(0x002a80b0, 0xe0 * SLOTS), range(0x002a5a80, 0x50 * SLOTS)] },
  { pc: pc(P.load), ranges: [range(0x002ad620, 0x100)] },
];

import fs from 'node:fs';
import { readTraceFor, GifPath, dataPath } from '../../../CrystalOSD/References/lib/index.mjs';
import { ICON_NAMES, cipherKit, decryptIcoImage, icoMembers, parseIco, vertexPacket } from '../../../CrystalOSD/References/model/browser_icons.mjs';
import { readElf } from '../../../CrystalOSD/References/scripts/extract_opening_vu1.mjs';
import { Vu1, feed, refWords } from './browser_icons_draw.mjs';
import { localMatrix, itemMatrix, CONSTANTS } from './browser_icons_matrix.mjs';
import { matrix } from '../../../CrystalOSD/References/model/opening-lib.mjs';

const DESCRIPTORS = 0x002a5a80;
const ROWS = 0x002ad620;

const bytesAt = (r, a, n) => { for (const m of r.mem) if (m.bytes && a >= m.address && a + n <= m.address + m.bytes.length) return m.bytes.subarray(a - m.address, a - m.address + n); throw new Error(`0x${a.toString(16)} is not in the probe at 0x${r.pc.toString(16)}`); };

// The 16 members, decoded from ICOIMAGE and parsed: their vertex packets.
function members() {
  const elf = fs.readFileSync(dataPath('dumps/hddosd-host/hddosd.elf'));
  const container = decryptIcoImage(fs.readFileSync(dataPath('dumps/hddosd-host/ICOIMAGE')), cipherKit(elf));
  return icoMembers(container).map((m) => { const ico = parseIco(m.data ?? m.bytes ?? m); return { name: m.name, ico, packet: vertexPacket(ico) }; });
}

export function verify(file, bend = null) {
  const trace = readTraceFor(file, PROBES);
  if (!trace.complete) throw new Error(`${file}: ${trace.reason}`);
  const icons = members();
  const read = readElf();
  const vu = new Vu1();
  const state = { base: 0, offset: 0, tops: 0, flip: 0 };
  const slots = new Map();
  const writesOf = (bytes) => [...new GifPath().feed(bytes)].filter((e) => e.kind === 'write');
  const sent = trace.packets.map((packet, at) => ({ packet, at })).filter(({ packet }) => packet.path === 1);
  let next = 0;
  const result = { draws: 0, packets: [0, 0], writes: [0, 0], vertices: 0, culled: 0, startAdc: 0, by: {}, locals: 0, matrices: 0, problems: [] };
  const constants = CONSTANTS(read);
  const kinds = new Map();
  const same = (a, b) => a.every((row, i) => row.every((v, k) => v === b[i][k]));
  let item = null;
  const problem = (t) => { if (result.problems.length < 14) result.problems.push(t); };
  for (const r of trace.probes) {
    if (r.preroll) continue;
    if (r.pc === P.load) {
      const rows = bytesAt(r, ROWS, 0x100);
      const at = r.gpr[5] >>> 0;
      const index = [...Array(16).keys()].find((i) => rows.readUInt32LE(16 * i + 4) === at);
      slots.set(r.gpr[4] >>> 0, index);
      continue;
    }
    // func_00259240: the item's local matrix (0x002593A8, sp) and matrix A (func_002567D0's a1).
    if (r.pc === P.item) { item = { record: Buffer.from(bytesAt(r, r.gpr[4] >>> 0, 0x6a0)), row: Buffer.from(bytesAt(r, r.gpr[5] >>> 0, 0x70)), frame: r.captureFrame }; continue; }
    if (r.pc === P.local && item) {
      const scale = Buffer.alloc(4); scale.writeUInt32LE(r.fpr[12] >>> 0);
      const kind = item.record.readInt32LE(0x130);
      kinds.set(kind, (kinds.get(kind) ?? 0) + 1);
      item.local = localMatrix(item.record, scale.readFloatLE(0) * (bend === 'scale' ? 1.001 : 1), constants);
      if (same(item.local, matrix(bytesAt(r, r.gpr[29] >>> 0, 0x40)))) result.locals += 1;
      else problem(`frame ${item.frame}: the local matrix of the item of kind ${item.record.readInt32LE(0x130)} differs`);
      continue;
    }
    if (r.pc === P.draw && item?.local) {
      const a = bend === 'order' ? itemMatrix(Buffer.concat([Buffer.alloc(0x30), Buffer.from(item.local.flat().map((v) => { const b = Buffer.alloc(4); b.writeFloatLE(v); return b; }).reduce((x, y) => Buffer.concat([x, y])))]), matrix(item.row, 0x30)) : itemMatrix(item.row, item.local);
      if (same(a, matrix(bytesAt(r, r.gpr[5] >>> 0, 0x40)))) result.matrices += 1;
      else problem(`frame ${item.frame}: matrix A differs`);
      item = null;
      continue;
    }
    if (r.pc !== P.kick) continue;
    const descriptor = r.gpr[5] >>> 0;
    const slot = (descriptor - DESCRIPTORS) / 0x50;
    const desc = bytesAt(r, descriptor, 0x50);
    const member = icons[slots.get(slot)];
    if (!member) { problem(`frame ${r.captureFrame}: slot ${slot} has no member loaded in the capture`); continue; }
    const program = { qwc: desc.readUInt32LE(0x10) & 0xffff, at: desc.readUInt32LE(0x14) };
    const block = { qwc: desc.readUInt32LE(0x20) & 0xffff, at: desc.readUInt32LE(0x24) };
    const vertices = { qwc: desc.readUInt32LE(0x30) & 0xffff };
    if (vertices.qwc * 4 !== member.packet.length) { problem(`frame ${r.captureFrame}: slot ${slot} sends ${vertices.qwc} qwords, ${member.name}'s packet is ${member.packet.length / 4}`); continue; }
    const words = [
      ...refWords(bytesAt(r, 0x002a7f30, 0x50), 5),
      ...refWords(bytesAt(r, 0x002a7f80, 0x90), 9),
      ...refWords(read(program.at, program.qwc * 16), program.qwc),
      ...refWords(bytesAt(r, block.at, block.qwc * 16), block.qwc),
      0, 0, ...member.packet,
    ];
    if (bend === 'bias') { const b = Buffer.from(bytesAt(r, block.at, block.qwc * 16)); b.writeFloatLE(b.readFloatLE(0x90) + 50, 0x90); words.splice(words.length - member.packet.length - 2 - block.qwc * 4 - 2, block.qwc * 4 + 2, ...refWords(b, block.qwc)); }
    vu.kicks = [];
    try { feed(vu, words, state); } catch (e) { problem(`frame ${r.captureFrame}: ${e.message}`); continue; }
    while (next < sent.length && sent[next].at < r.at) next += 1;
    if (next + vu.kicks.length > sent.length) break;
    result.draws += 1;
    result.by[member.name] = (result.by[member.name] ?? 0) + 1;
    vu.kicks.forEach((bytes, k) => {
      const expected = writesOf(bytes), got = writesOf(sent[next + k].packet.bytes);
      result.packets[1] += 1;
      result.writes[1] += expected.length;
      let equal = got.length === expected.length, bad = null;
      // Only every third vertex of a chunk closes a triangle (chunks are whole triangles); the ADC bit of the other two changes
      // nothing the GS draws and is counted apart (Open: the MAC flag FMAND reads in the first loop pass after a program start,
      // and the sign of an exact zero cross product, in the emulator the captures come from).
      const xyz = (w) => w.reg === 4 || w.reg === 0xc;
      let vertex = -1;
      expected.forEach((w, i) => {
        if (xyz(w)) vertex += 1;
        const same = got[i] && got[i].reg === w.reg && got[i].value === w.value;
        const startOnly = !same && got[i] && xyz(w) && xyz(got[i]) && got[i].value === w.value && vertex % 3 !== 2;
        if (same || startOnly) { result.writes[0] += 1; if (startOnly) result.startAdc += 1; } else { equal = false; bad ??= i; }
      });
      if (equal) result.packets[0] += 1;
      else problem(`frame ${r.captureFrame} slot ${slot} ${member.name} packet ${k}: ${got.length} writes sent, ${expected.length} computed${bad === null ? '' : `; write ${bad}: sent ${got[bad] ? `reg 0x${got[bad].reg.toString(16)} = 0x${got[bad].value.toString(16)}` : 'nothing'}, computed reg 0x${expected[bad].reg.toString(16)} = 0x${expected[bad].value.toString(16)}`}`);
      result.vertices += expected.filter((w) => w.reg === 4 || w.reg === 0xc).length;
      result.culled += expected.filter((w) => w.reg === 0xc).length;
    });
    next += vu.kicks.length;
  }
  result.kinds = Object.fromEntries(kinds);
  return result;
}

if (process.argv[1] && process.argv[1].endsWith('verify_browser_icons.mjs')) {
  const file = process.argv[2];
  const result = verify(file);
  console.log(`local matrices equal ${result.locals} (item kinds at +0x130: ${JSON.stringify(result.kinds)}), matrices A equal ${result.matrices}`);
  console.log(`icon draws ${result.draws} ${JSON.stringify(result.by)}; packets equal ${result.packets[0]} of ${result.packets[1]} (writes ${result.writes[0]} of ${result.writes[1]}); vertices ${result.vertices}, sent without drawing ${result.culled}; ADC bits that differ on vertices closing no triangle (Open) ${result.startAdc}`);
  for (const t of result.problems) console.log(`  ! ${t}`);
  const controls = ['bias', 'scale', 'order'].map((b) => [b, verify(file, b).problems.length]);
  console.log(`controls (a rule bent must fail): ${controls.map(([b, n]) => `${b} ${n}`).join(', ')}`);
  const ok = result.draws > 0 && result.locals > 0 && result.matrices > 0 && result.problems.length === 0 && result.packets[0] === result.packets[1] && controls.every(([, n]) => n > 0);
  console.log(`verdict: ${ok ? `FOUND ${result.draws} icon draws, every write equal` : 'PARTIAL see the lines marked !'}`);
  process.exit(ok ? 0 : 3);
}
