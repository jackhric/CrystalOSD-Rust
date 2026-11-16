// The Browser's icon draw on VU1 (HDD OSD 1.10U): the VIF1 words an icon's chain sends, and the microprogram run on them as an
// interpreter of its own instructions (read from the program image), for the instructions it uses. The data memory and the
// registers persist from one draw to the next, as on the console.
//
// Timing: an instruction issues one cycle after the previous one (the program is scheduled so that no register it reads is
// still being written); DIV's Q is the new quotient from the seventh cycle after it issues, the old one before; the MAC flag that
// FMAND reads is the one of the FMAC instruction issued four cycles before (MAX, MINI, FTOI, MOVE do not set it).
//
// Arithmetic: products and sums as the opening's VU1 verifier (../model/opening-lib.mjs: f, add, sub, quotient).
import { f, add, sub, quotient, toInt } from '../../../CrystalOSD/References/model/opening-lib.mjs';
import { upper, lower } from '../../../CrystalOSD/References/scripts/extract_opening_vu1.mjs';

const FIELDS = ['x', 'y', 'z', 'w'];
const toBits = (x) => { const b = Buffer.alloc(4); b.writeFloatLE(x); return b.readUInt32LE(0); };
const fromBits = (u) => { const b = Buffer.alloc(4); b.writeUInt32LE(u >>> 0); return b.readFloatLE(0); };

// The instructions of a microprogram sent with MPG: [{ upper, lower, up, low, immediate }], by address.
export function decodeProgram(words) {
  const code = [];
  for (let i = 0; i < words.length; i += 2) {
    const low = words[i] >>> 0, high = words[i + 1] >>> 0;
    const immediate = (high >>> 31) === 1;
    code.push({ high, low, up: upper(high), lo: immediate ? null : { ...lower(low, i / 2), pc: i / 2, word: low }, immediate });
  }
  return code;
}

export class Vu1 {
  constructor() {
    this.memory = Buffer.alloc(0x4000);
    this.vf = Array.from({ length: 32 }, () => [0, 0, 0, 0]);
    this.vf[0] = [0, 0, 0, 1];
    this.vi = new Array(16).fill(0);
    this.acc = [0, 0, 0, 0];
    this.i = 0;
    this.q = { value: 0, pending: null };
    this.mac = [];
    this.cycle = 0;
    this.tpc = 0;
    this.code = [];
    this.kicks = [];
    this.top = 0;
    this.ints = new Array(32).fill(false);
  }

  vector(n) { return [0, 4, 8, 12].map((k) => this.memory.readFloatLE(((n & 0x3ff) << 4) + k)); }
  word(n, k) { return this.memory.readUInt32LE(((n & 0x3ff) << 4) + k * 4); }

  qNow() {
    if (this.q.pending && this.cycle >= this.q.pending.ready) { this.q.value = this.q.pending.value; this.q.pending = null; }
    return this.q.value;
  }

  macNow() {
    let flag = 0;
    for (const m of this.mac) if (m.ready <= this.cycle) flag = m.flag;
    this.mac = this.mac.filter((m) => m.ready > this.cycle || m.flag === flag);
    return flag;
  }

  setMac(result, dest) {
    let flag = 0;
    FIELDS.forEach((name, k) => {
      if (!dest.includes(name)) return;
      const bit = 3 - k;
      if (result[k] === 0) flag |= 1 << bit;
      if (result[k] < 0) flag |= 1 << (bit + 4);
    });
    this.mac.push({ ready: this.cycle + 4, flag });
  }

  // A register FTOI wrote holds integers (`ints`); stores write them as integers.
  write(fd, dest, values, integer = false) {
    if (fd === 0) return;
    const r = this.vf[fd].slice();
    FIELDS.forEach((name, k) => { if (dest.includes(name)) r[k] = values[k]; });
    this.vf[fd] = r;
    this.ints[fd] = integer;
  }

  // One upper instruction; returns nothing. `bc` is the broadcast field of the vf[ft] operand.
  runUpper(u) {
    const { name, ft, fs, fd, dest, bc } = u;
    if (name === 'NOP') return;
    const a = this.vf[fs], b = this.vf[ft];
    const scalar = bc ? b[FIELDS.indexOf(bc)] : null;
    const each = (fn) => [0, 1, 2, 3].map((k) => (dest.includes(FIELDS[k]) ? fn(k) : 0));
    const base = name.replace(/[xyzw]$/, '');
    const operand = (k) => (/[xyzw]$/.test(name) && !/^(MAX|MINI)$/.test(name) ? scalar : name.endsWith('q') ? this.qNow() : name.endsWith('i') && name !== 'MINI' && name !== 'MULi' ? this.i : b[k]);
    let result;
    let flags = true;
    switch (base) {
      case 'MULA': this.acc = each((k) => f(a[k] * operand(k))).map((v, k) => (dest.includes(FIELDS[k]) ? v : this.acc[k])); this.setMac(this.acc, dest); return;
      case 'MADDA': this.acc = each((k) => add(this.acc[k], f(a[k] * operand(k)))).map((v, k) => (dest.includes(FIELDS[k]) ? v : this.acc[k])); this.setMac(this.acc, dest); return;
      case 'MADD': result = each((k) => add(this.acc[k], f(a[k] * operand(k)))); break;
      case 'MUL': result = each((k) => f(a[k] * operand(k))); break;
      case 'MULq': { const q = this.qNow(); result = each((k) => f(a[k] * q)); break; }
      case 'MINIi': result = each((k) => Math.min(a[k], this.i)); flags = false; break;
      case 'MAX': result = each((k) => Math.max(a[k], scalar ?? b[k])); flags = false; break;
      case 'SUB': result = each((k) => sub(a[k], operand(k))); break;
      case 'FTOI0': this.write(ft, dest, each((k) => toInt(a[k])), true); return;
      case 'FTOI4': this.write(ft, dest, each((k) => toInt(f(a[k] * 16))), true); return;
      case 'OPMULA': this.acc = [f(a[1] * b[2]), f(a[2] * b[0]), f(a[0] * b[1]), this.acc[3]]; this.setMac(this.acc, 'xyz'); return;
      case 'OPMSUB': result = [sub(this.acc[0], f(a[1] * b[2])), sub(this.acc[1], f(a[2] * b[0])), sub(this.acc[2], f(a[0] * b[1])), 0]; break;
      default: throw new Error(`upper ${u.text} is not modelled`);
    }
    if (flags) this.setMac(result, dest);
    this.write(fd, dest, result);
  }

  // One lower instruction; returns { branch: target } when it branches.
  runLower(l, immediateWord) {
    if (l === null) { this.i = fromBits(immediateWord); return null; }
    const { name, ft, fs, dest, imm11, imm15, target } = l;
    const vi = this.vi;
    const set = (r, v) => { if (r !== 0) vi[r] = v & 0xffff; };
    const s16 = (v) => (v << 16) >> 16;
    switch (name) {
      case 'MOVE': this.write(ft, dest, this.vf[fs], this.ints[fs]); return null;
      case 'IADDIU': set(ft, vi[fs] + imm15); return null;
      case 'IADDI': set(ft, vi[fs] + ((l.fd << 27) >> 27)); return null;
      case 'IAND': set(l.fd, vi[fs] & vi[ft]); return null;
      case 'LQ': this.write(ft, dest, this.vector(s16(vi[fs]) + imm11)); return null;
      case 'LQI': this.write(ft, dest, this.vector(vi[fs])); set(fs, vi[fs] + 1); return null;
      case 'SQ': (this.ints[fs] ? this.storeInts : this.store).call(this, s16(vi[ft]) + imm11, this.vf[fs], dest); return null;
      case 'SQI': (this.ints[fs] ? this.storeInts : this.store).call(this, vi[ft], this.vf[fs], dest); set(ft, vi[ft] + 1); return null;
      case 'ISW': { const at = (s16(vi[fs]) + imm11) & 0x3ff; FIELDS.forEach((n, k) => { if (dest.includes(n)) this.memory.writeUInt32LE(vi[ft] & 0xffff, (at << 4) + 4 * k); }); return null; }
      case 'ILWR': { const k = FIELDS.indexOf(dest); set(ft, this.word(vi[fs], k) & 0xffff); return null; }
      case 'XTOP': set(ft, this.top); return null;
      case 'XGKICK': this.kick(vi[fs]); return null;
      case 'DIV': {
        const num = this.vf[fs][(l.word >>> 21) & 3];
        const den = this.vf[ft][(l.word >>> 23) & 3];
        this.qNow();
        this.q.pending = { ready: this.cycle + 7, value: quotient(num, den) };
        return null;
      }
      case 'FMAND': set(ft, this.macNow() & vi[fs]); return null;
      case 'FMOR': set(ft, this.macNow() | vi[fs]); return null;
      case 'B': return { branch: target };
      case 'BAL': set(ft, l.pc + 2); return { branch: target };
      case 'JALR': set(ft, l.pc + 2); return { branch: vi[fs] };
      case 'JR': return { branch: vi[fs] };
      case 'IBNE': return vi[ft] !== vi[fs] ? { branch: target } : null;
      case 'IBEQ': return vi[ft] === vi[fs] ? { branch: target } : null;
      default: throw new Error(`lower ${l.text} is not modelled`);
    }
  }

  store(n, v, dest) {
    const at = (n & 0x3ff) << 4;
    FIELDS.forEach((name, k) => { if (dest.includes(name)) this.memory.writeFloatLE(v[k], at + 4 * k); });
  }

  storeInts(n, v, dest) {
    const at = (n & 0x3ff) << 4;
    FIELDS.forEach((name, k) => { if (dest.includes(name)) this.memory.writeInt32LE(v[k] | 0, at + 4 * k); });
  }

  // XGKICK: the GIF packet from an entry to the end of its last tag.
  kick(n) {
    let at = (n & 0x3ff) << 4;
    const from = at;
    for (;;) {
      const loops = this.memory.readUInt32LE(at) & 0x7fff, end = (this.memory.readUInt32LE(at) >>> 15) & 1;
      const flag = (this.memory.readUInt32LE(at + 4) >>> 26) & 3, registers = (this.memory.readUInt32LE(at + 4) >>> 28) || 16;
      at += 16 + (flag === 0 ? loops * registers * 16 : flag === 1 ? Math.ceil((loops * registers) / 2) * 16 : loops * 16);
      if (end) break;
    }
    this.kicks.push(Buffer.from(this.memory.subarray(from, at)));
  }

  // From `pc` until the instruction after one with the E bit.
  run(pc) {
    let delay = null;
    let ending = false;
    for (let guard = 0; guard < 200000; guard++) {
      const ins = this.code[pc];
      if (!ins) throw new Error(`no instruction at ${pc}`);
      // Upper and lower read their operands at issue: the lower one runs first unless it reads what the upper one writes.
      const jump = this.bundle(ins);
      this.cycle += 1;
      let next = pc + 1;
      if (delay !== null) { next = delay; delay = null; }
      if (jump && jump.branch !== undefined) delay = jump.branch;
      if (ending) { this.tpc = next; return; }
      if (ins.up.flags.includes('E')) ending = true;
      pc = next;
    }
    throw new Error('the program does not stop');
  }

  // Both halves of an instruction read their registers before either writes: the lower half on a copy of the registers when the
  // upper half writes one it reads.
  bundle(ins) {
    const before = this.vf.slice();
    const kinds = this.ints.slice();
    this.runUpper(ins.up);
    const after = this.vf;
    const afterKinds = this.ints;
    this.vf = before.map((v, k) => (after[k] !== before[k] ? before[k] : v));
    this.ints = kinds;
    const jump = this.runLower(ins.lo, ins.low);
    for (let k = 1; k < 32; k++) {
      if (after[k] !== before[k]) { this.vf[k] = after[k]; this.ints[k] = afterKinds[k]; }
    }
    return jump;
  }
}

// What VIF1 does with the words of a draw: NOP, STCYCL (4, 4), BASE, OFFSET, UNPACK V4-32 (with TOPS when bit 15 is set), MPG
// (the program, decoded), MSCAL, MSCNT.
export function feed(vu, words, state) {
  const start = () => {
    vu.top = state.tops;
    state.tops = state.flip === 0 ? state.base + state.offset : state.base;
    state.flip ^= 1;
  };
  for (let i = 0; i < words.length; i++) {
    const code = words[i] >>> 0, command = (code >>> 24) & 0x7f, number = (code >>> 16) & 0xff, immediate = code & 0xffff;
    if (command === 0x00 || command === 0x01) continue;
    if (command === 0x03) { state.base = immediate & 0x3ff; continue; }
    if (command === 0x02) { state.offset = immediate & 0x3ff; state.flip = 0; state.tops = state.base; continue; }
    if (command === 0x4a) {
      const count = number === 0 ? 256 : number;
      const program = decodeProgram(words.slice(i + 1, i + 1 + 2 * count));
      program.forEach((ins, k) => { vu.code[immediate + k] = ins; });
      i += 2 * count;
      continue;
    }
    if (command === 0x14) { start(); vu.run(immediate); continue; }
    if (command === 0x17) { start(); vu.run(vu.tpc); continue; }
    if (command === 0x6c) {
      const address = (immediate & 0x3ff) + ((immediate & 0x8000) ? state.tops : 0);
      const entries = number === 0 ? 256 : number;
      for (let k = 0; k < entries * 4; k++) vu.memory.writeUInt32LE(words[i + 1 + k] >>> 0, ((address << 4) + k * 4) & 0x3fff);
      i += entries * 4;
      continue;
    }
    throw new Error(`VIF code 0x${code.toString(16)} is not handled`);
  }
}

// The VIF words of a REF of `qwc` quadwords at `bytes` (the tag's upper half is two NOPs).
export function refWords(bytes, qwc) {
  const words = [0, 0];
  for (let k = 0; k < qwc * 4; k++) words.push(bytes.readUInt32LE(4 * k));
  return words;
}

export { toBits, fromBits };
