use super::memory::IopMemory;
use super::registers::reg;
use super::state::{BANKS, CTX, SLOTS, State, VOICES, div60, i32, m, mul_hi, s16};
use crate::{Result, fail};

const SQ: i64 = m(0x9280);
const PROG_TAB: i64 = m(0x9284);
const PROG_TAB2: i64 = m(0x9288);
const VEL_TAB: i64 = m(0x928c);
const HD: i64 = m(0x9298);
const PROG_HDR: i64 = m(0x9220);
const TONE: i64 = m(0x9224);
const SQ2: i64 = m(0x9218);
const CHAN: i64 = m(0x9214);
const PITCH_TABLE: i64 = m(0x8a30);
const PAN_TABLE: i64 = m(0x8ef0);
const SLOT_SIZE: i64 = 0x44;
const VOICE_SIZE: i64 = 0x3c;
const VOICE_COUNT: i32 = 24;
const SEQ_CORE: i64 = 1;

fn div48000(x: i64) -> i32 {
    let v = i32(x);
    (mul_hi(i64::from(v), 0x057619f1) >> 10).wrapping_sub(v >> 31)
}

fn imul(a: i64, b: i64) -> i32 {
    i32(((a as u64 as u32 as u64).wrapping_mul(b as u64 as u32 as u64)) as i64)
}

fn sequencer_pitch(mem: &IopMemory, root: u32, key: u32, fine: i32, bend: u32, range: u32) -> Result<i32> {
    let table = |index: i64| mem.u16(PITCH_TABLE + 2 * (index & 0xffff));
    let bend_term = imul(i64::from(bend) - 0x40, i64::from(range)) >> 2;
    if key >= root {
        let d = (i64::from(key) - i64::from(root)) & 0xffff;
        let octave = d / 12;
        let semitone = d - 12 * octave;
        let index = (semitone << 4) + i64::from(bend_term.wrapping_add(0xd0)) + i64::from(s16(i64::from(fine)));
        let x = imul(i64::from(table(index)?), 44100);
        return Ok(div48000(i64::from(i32(i64::from((x as u32) << (octave & 31))))) & 0xffff);
    }
    let d = (i64::from(root) - i64::from(key)) & 0xffff;
    let octave = d / 12;
    let semitone = d - 12 * octave;
    let index = ((12 - semitone) << 4) + i64::from(bend_term.wrapping_add(0xd0)) + i64::from(s16(i64::from(fine)));
    let x = imul(i64::from(table(index)?), 44100);
    Ok(div48000(i64::from(x >> ((octave + 1) & 31))) & 0xffff)
}

fn slot_at(i: i32) -> i64 {
    SLOTS + SLOT_SIZE * i64::from(i)
}

fn voice_at(j: i32) -> i64 {
    VOICES + VOICE_SIZE * i64::from(j)
}

struct Sequencer<'a> {
    s: &'a mut State,
}

impl Sequencer<'_> {
    fn run(&mut self) -> Result<()> {
        self.release_finished()?;
        let mut changed = 0;
        for i in 0..0x18 {
            let slot = slot_at(i);
            if self.s.mem.u16(slot + 0x26)? == 1 && self.s.mem.u16(slot + 0x28)? == 1 {
                self.select_bank(i)?;
                while self.s.mem.s32(slot + 4)? <= 0 {
                    self.read_event(i)?;
                    let r = self.prepare_event(i)?;
                    if r == 1 {
                        changed = 1;
                        let status = u32::from(self.s.mem.u8(slot)?) & 0xf0;
                        if status == 0xb0 {
                            self.controller(i)?;
                        } else if status == 0x90 {
                            self.note_on(i)?;
                        } else if status == 0x80 {
                            self.note_off(i)?;
                        } else if status == 0xa0 {
                        } else if status == 0xe0 {
                            return fail("pitch bend is not modelled (SNDCLOKS does not use it)");
                        } else if status == 0xc0 {
                            self.program_change(i)?;
                        } else if status == 0xf0 {
                            let kind = u32::from(self.s.mem.u8(slot + 2)?);
                            if kind == 0x2f {
                                self.end_of_sequence(i)?;
                                break;
                            }
                            if kind == 0x51 {
                                self.tempo(i)?;
                            }
                        }
                    } else {
                        changed = 0;
                    }
                    self.read_delta(i)?;
                }
                let mem = &mut self.s.mem;
                if changed == 1 {
                    mem.write32(CTX + 4, i64::from(mem.u32(CTX + 4)? | mem.u32(slot + 0xc)?))?;
                    mem.write32(CTX + 8, i64::from(mem.u32(CTX + 8)? | mem.u32(slot + 0x10)?))?;
                    mem.write32(slot + 0x10, 0)?;
                    mem.write32(slot + 0xc, 0)?;
                    changed = 0;
                }
                if mem.u16(slot + 0x3e)? != 0 {
                    mem.write32(slot + 4, i64::from(mem.u32(slot + 4)?) - i64::from(mem.u32(slot + 8)?))?;
                }
                if mem.u16(slot + 0x2c)? == 1 {
                    mem.write8(slot, i64::from(mem.u8(slot + 0x2e)?))?;
                    mem.write8(slot + 1, i64::from(mem.u8(slot + 0x2e)?))?;
                    mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x38)?))?;
                    mem.write16(slot + 0x2c, 0)?;
                    mem.write16(slot + 0x28, 1)?;
                    mem.write32(slot + 4, 0)?;
                }
            }
        }
        let off = self.s.mem.u32(CTX + 8)?;
        if off != 0 {
            self.s.set_switch(SEQ_CORE, reg::KOFF0, off)?;
        }
        let on = self.s.mem.u32(CTX + 4)?;
        if on != 0 {
            self.s.set_switch(SEQ_CORE, reg::KON0, on)?;
        }
        self.s.mem.write32(CTX + 4, 0)?;
        self.s.mem.write32(CTX + 8, 0)?;
        self.voice_updates()
    }

    fn voice_param(&mut self, voice: i32, param: u32, value: i64) -> Result<()> {
        self.s.set_param(SEQ_CORE, i64::from(voice), param, value)
    }

    fn select_bank(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        mem.write32(SQ, i64::from(mem.u32(slot + 0x18)?))?;
        let hd = i64::from(mem.u32(BANKS + 12 * i64::from(mem.u16(slot + 0x24)?))?);
        mem.write32(CTX, hd)?;
        mem.write32(HD, hd)?;
        mem.write32(PROG_TAB, hd + i64::from(mem.u32(hd + 0x10)?))?;
        mem.write32(PROG_TAB2, hd + i64::from(mem.u32(hd + 0x10)?))?;
        mem.write32(VEL_TAB, hd + i64::from(mem.u32(hd + 0x14)?))
    }

    fn read_event(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let first = u32::from(mem.u8(i64::from(mem.u32(SQ)?) + i64::from(mem.u32(slot + 0x14)?))?);
        if first & 0x80 != 0 {
            mem.write8(slot + 1, i64::from(first))?;
            mem.write8(slot, i64::from(first))?;
        } else {
            mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) - 1)?;
            mem.write8(slot, i64::from(mem.u8(slot + 1)?))?;
        }
        let base = i64::from(mem.u32(SQ)?) + i64::from(mem.u32(slot + 0x14)?);
        mem.write8(slot + 2, i64::from(mem.u8(base + 1)?))?;
        mem.write8(slot + 3, i64::from(mem.u8(base + 2)?))
    }

    fn prepare_event(&mut self, i: i32) -> Result<i32> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let channel = i64::from(mem.u8(slot)? & 0xf);
        let sq = i64::from(mem.u32(SQ)?);
        let program = u32::from(mem.u8(sq + (channel << 4) + 0x12)?);
        let entry = u32::from(mem.u16(i64::from(mem.u32(PROG_TAB)?) + 2 * i64::from(program) + 2)?);
        if mem.u8(slot)? < 0xa0 {
            let bad = entry == 0xffff || u32::from(mem.u16(i64::from(mem.u32(PROG_TAB)?))?) < program || mem.u32(i64::from(mem.u32(HD)?) + 0x10)? == 0xffffffff;
            if bad {
                mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)?;
                return Ok(0);
            }
        }
        let hd = i64::from(mem.u32(CTX)?);
        let prog_hdr = i64::from(mem.u32(i64::from(mem.u32(HD)?) + 0x10)?) + hd + i64::from(entry);
        mem.write32(PROG_HDR, prog_hdr)?;
        mem.write32(TONE, prog_hdr + 8)?;
        mem.write32(SQ2, i64::from(mem.u32(slot + 0x18)?))?;
        mem.write32(CHAN, i64::from(mem.u32(slot + 0x18)?) + (channel << 4) + 0x10)?;
        Ok(1)
    }

    fn read_delta(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let mut delta = 0u32;
        loop {
            let b = u32::from(mem.u8(i64::from(mem.u32(SQ)?) + i64::from(mem.u32(slot + 0x14)?))?);
            mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 1)?;
            delta = (delta << 7) | (b & 0x7f);
            if b & 0x80 == 0 {
                break;
            }
        }
        if mem.u16(slot + 0x3e)? != 0 {
            mem.write32(slot + 4, i64::from(mem.u32(slot + 4)?) + i64::from(delta << 12))?;
        }
        Ok(())
    }

    fn allocate_voice(&mut self) -> Result<i32> {
        let mem = &mut self.s.mem;
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if mem.u16(v)? == 0 && mem.u16(v + 2)? == 0 {
                return Ok(j);
            }
        }
        let mut best = VOICE_COUNT;
        let mut age = 0x18u32;
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if mem.u16(v + 8)? == 1 && u32::from(mem.u16(v + 0xa)?) < age {
                age = u32::from(mem.u16(v + 0xa)?);
                best = j;
            }
        }
        if best == VOICE_COUNT {
            return Ok(-1);
        }
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if age < u32::from(mem.u16(v + 0xa)?) {
                mem.write16(v + 0xa, i64::from(mem.u16(v + 0xa)?) - 1)?;
            }
        }
        Ok(best)
    }

    fn tone_matches(&self, head: u32, t: i64, key: u32) -> Result<bool> {
        if head == 0xff {
            return Ok(true);
        }
        let mem = &self.s.mem;
        let tone = i64::from(mem.u32(TONE)?) + (t << 4);
        Ok(key >= u32::from(mem.u8(tone)?) && key <= u32::from(mem.u8(tone + 1)?))
    }

    fn voice_volume(&self, j: i32, side: i32) -> Result<i32> {
        voice_volume_of(&self.s.mem, j, side)
    }

    fn note_off(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if mem.u16(v)? == 1
                && u32::from(mem.u16(v + 6)?) == i as u32
                && mem.u16(v + 2)? == u16::from(mem.u8(slot + 2)?)
                && mem.u16(v + 0x1c)? == mem.u16(slot + 0x24)?
                && u32::from(mem.u16(v + 4)?) == u32::from(mem.u8(slot)?) & 0xf
            {
                mem.write16(v + 8, 1)?;
                mem.write32(slot + 0x10, i64::from(mem.u32(slot + 0x10)? | (1u32 << j)))?;
            }
        }
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)
    }

    fn note_on(&mut self, i: i32) -> Result<()> {
        let slot = slot_at(i);
        if self.s.mem.u8(slot + 3)? == 0 {
            return self.note_off(i);
        }
        let tone0 = i64::from(self.s.mem.u32(PROG_HDR)?);
        let head = u32::from(self.s.mem.u8(tone0)?);
        let first: i64;
        let last: i64;
        let single: bool;
        if head == 0xff {
            last = i64::from(self.s.mem.u8(slot + 2)?) - i64::from(self.s.mem.u8(tone0 + 6)?);
            first = last;
            single = false;
        } else if head & 0x80 != 0 {
            last = i64::from(head) - 0x80;
            single = false;
            first = 0;
        } else {
            last = i64::from(head);
            single = true;
            first = 0;
        }
        let mut t = first;
        while t < last + 1 {
            if !self.tone_matches(head, t, u32::from(self.s.mem.u8(slot + 2)?))? {
                t += 1;
                continue;
            }
            let voice = self.allocate_voice()?;
            if voice == -1 {
                break;
            }
            let mem = &mut self.s.mem;
            mem.write32(TONE, i64::from(mem.u32(TONE)?) + (t << 4))?;
            let v = voice_at(voice);
            let tone = i64::from(mem.u32(TONE)?);
            let chan = i64::from(mem.u32(CHAN)?);
            let flags = u32::from(mem.u8(tone + 0xf)?);
            if flags & 1 != 0 {
                mem.write16(v + 8, 0)?;
                mem.write16(v + 0xc, 1)?;
            } else {
                mem.write16(v + 8, 1)?;
                mem.write16(v + 0xc, 0)?;
            }
            mem.write16(v, 1)?;
            mem.write16(v + 2, i64::from(mem.u8(slot + 2)?))?;
            mem.write16(v + 4, i64::from(mem.u8(slot)? & 0xf))?;
            mem.write16(v + 6, i64::from(i))?;
            mem.write16(v + 0xa, i64::from(mem.u16(CTX + 0xc)?))?;
            mem.write16(v + 0xe, t)?;
            mem.write16(v + 0x12, 0)?;
            mem.write16(v + 0x1c, i64::from(mem.u16(slot + 0x24)?))?;
            mem.write16(v + 0x1e, i64::from(mem.u8(i64::from(mem.u32(PROG_HDR)?) + 1)?))?;
            mem.write16(v + 0x20, i64::from(mem.u8(i64::from(mem.u8(slot + 3)?) + i64::from(mem.u32(VEL_TAB)?) + 2)?))?;
            mem.write16(v + 0x22, i64::from(mem.u8(tone + 0xb)?))?;
            let pan = 0.max(0x7f.min(s16(i64::from(mem.u8(chan + 4)?) + i64::from(mem.u8(tone + 0xc)?) - 0x40)));
            mem.write16(v + 0x24, i64::from(mem.u8(PAN_TABLE + 2 * i64::from(pan >> 2))?))?;
            mem.write16(v + 0x26, i64::from(mem.u8(PAN_TABLE + 2 * i64::from(pan >> 2) + 1)?))?;
            mem.write16(v + 0x28, i64::from(mem.u8(chan + 3)?))?;
            mem.write16(v + 0x2a, i64::from(mem.u8(i64::from(mem.u32(SQ2)?))?))?;
            let fine = u32::from(mem.u8(tone + 3)?);
            let fine_pitch = fine | if fine & 0x80 != 0 { 0xff00 } else { 0 };
            mem.write16(v + 0x2c, i64::from(fine_pitch))?;
            mem.write16(v + 0x2e, i64::from(mem.u8(chan + 0xa)?))?;
            mem.write16(v + 0x30, i64::from(mem.u8(tone + 0xd)?))?;
            mem.write16(v + 0x32, i64::from(mem.u8(chan + 0xc)?))?;
            mem.write16(v + 0x34, i64::from(mem.u8(tone + 2)?))?;
            mem.write16(v + 0x36, i64::from(mem.u8(tone + 0xa)?))?;
            mem.write16(v + 0x38, i64::from(mem.u8(chan + 4)?))?;
            mem.write16(v + 0x3a, i64::from(mem.u8(tone + 0xc)?))?;
            if flags & 0x20 != 0 && mem.u8(chan + 9)? != 0 {
                mem.write16(v + 0x10, i64::from(mem.u8(tone + 0xe)?))?;
                mem.write16(v + 0x14, 1)?;
                mem.write16(v + 0x16, i64::from(mem.u8(chan + 9)?))?;
            } else {
                mem.write16(v + 0x14, 0)?;
                mem.write16(v + 0x16, 0)?;
            }
            let bit = 1u32 << voice;
            let pitch = sequencer_pitch(
                mem,
                u32::from(mem.u8(tone + 2)?),
                u32::from(mem.u8(slot + 2)?),
                s16(i64::from(fine_pitch)),
                u32::from(mem.u8(chan + 0xa)?),
                u32::from(mem.u8(tone + 0xd)?),
            )?;
            let left = self.voice_volume(voice, 0)?;
            let right = self.voice_volume(voice, 1)?;
            let mem = &self.s.mem;
            let address = ((i64::from(mem.u16(tone + 4)?) + i64::from(mem.u32(BANKS + 12 * i64::from(mem.u16(slot + 0x24)?) + 4)?)) as u32) << 3;
            let adsr1 = mem.u16(tone + 6)?;
            let adsr2 = mem.u16(tone + 8)?;
            self.voice_param(voice, reg::VOLL, i64::from(left))?;
            self.voice_param(voice, reg::VOLR, i64::from(right))?;
            self.voice_param(voice, reg::PITCH, i64::from(pitch))?;
            self.s.set_addr(SEQ_CORE, i64::from(voice), address)?;
            self.voice_param(voice, reg::ADSR1, i64::from(adsr1))?;
            self.voice_param(voice, reg::ADSR2, i64::from(adsr2))?;
            let s = &mut *self.s;
            if flags & 0x80 != 0 {
                s.set_switch(SEQ_CORE, reg::VMIXEL0, s.get_switch(SEQ_CORE, reg::VMIXEL0)? | bit)?;
                s.set_switch(SEQ_CORE, reg::VMIXER0, s.get_switch(SEQ_CORE, reg::VMIXER0)? | bit)?;
            } else {
                s.set_switch(SEQ_CORE, reg::VMIXEL0, s.get_switch(SEQ_CORE, reg::VMIXEL0)? ^ bit)?;
                s.set_switch(SEQ_CORE, reg::VMIXER0, s.get_switch(SEQ_CORE, reg::VMIXER0)? ^ bit)?;
            }
            let mem = &mut s.mem;
            mem.write32(slot + 0xc, i64::from(mem.u32(slot + 0xc)? | bit))?;
            let count = mem.u16(CTX + 0xc)?;
            mem.write16(CTX + 0xc, i64::from(count) + i64::from(count < 0x18))?;
            mem.write32(TONE, i64::from(mem.u32(TONE)?) - (t << 4))?;
            if single {
                break;
            }
            t += 1;
        }
        self.s.mem.write32(slot + 0x14, i64::from(self.s.mem.u32(slot + 0x14)?) + 3)
    }

    fn end_of_sequence(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        mem.write32(slot + 0x14, 0x110)?;
        mem.write16(slot + 0x28, 0)?;
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if u32::from(mem.u16(v + 6)?) == i as u32 {
                mem.write16(v + 0x14, 0)?;
                mem.write16(v + 0x2e, 0x40)?;
            }
        }
        mem.write8(slot + 1, i64::from(mem.u8(slot)?))
    }

    fn tempo(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let ev = i64::from(mem.u32(slot + 0x18)?) + i64::from(mem.u32(slot + 0x14)?);
        mem.write16(slot + 0x3e, (i64::from(mem.u8(ev + 3)?) << 8) | i64::from(mem.u8(ev + 2)?))?;
        let product = (u32::from(mem.u16(slot + 0x40)?).wrapping_mul(u32::from(mem.u16(slot + 0x3e)?)) << 12) as i32;
        let hz = u32::from(mem.u16(CTX + 0xe)?);
        if hz == 0 {
            return fail("tick rate 0");
        }
        let divided = product / hz as i32;
        mem.write32(slot + 8, i64::from(div60(i64::from(divided))))?;
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 4)
    }

    fn program_change(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let chan = i64::from(mem.u32(CHAN)?);
        mem.write8(chan + 2, i64::from(mem.u8(i64::from(mem.u32(slot + 0x14)?) + i64::from(mem.u32(SQ)?) + 1)?))?;
        mem.write8(chan + 0xa, 0x40)?;
        mem.write8(chan + 0xb, 0x40)?;
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 2)
    }

    fn controller_volume(&mut self, i: i32) -> Result<()> {
        let slot = slot_at(i);
        let mem = &mut self.s.mem;
        let chan = i64::from(mem.u32(CHAN)?);
        let value = mem.u8(i64::from(mem.u32(slot + 0x14)?) + i64::from(mem.u32(SQ)?) + 2)?;
        mem.write8(chan + 3, i64::from(value))?;
        for j in 0..VOICE_COUNT {
            let mem = &mut self.s.mem;
            let v = voice_at(j);
            if mem.u16(v)? == 1
                && mem.u16(v + 8)? != 1
                && u32::from(mem.u16(v + 6)?) == i as u32
                && mem.u16(v + 0x1c)? == mem.u16(slot + 0x24)?
                && u32::from(mem.u16(v + 4)?) == u32::from(mem.u8(slot)?) & 0xf
            {
                mem.write16(v + 0x28, i64::from(mem.u8(i64::from(mem.u32(slot + 0x14)?) + i64::from(mem.u32(SQ)?) + 2)?))?;
                let left = self.voice_volume(j, 0)?;
                self.voice_param(j, reg::VOLL, i64::from(left))?;
                let right = self.voice_volume(j, 1)?;
                self.voice_param(j, reg::VOLR, i64::from(right))?;
            }
        }
        let mem = &mut self.s.mem;
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)
    }

    fn controller_pan(&mut self, i: i32) -> Result<()> {
        let slot = slot_at(i);
        let mem = &mut self.s.mem;
        let chan = i64::from(mem.u32(CHAN)?);
        let value = mem.u8(i64::from(mem.u32(slot + 0x14)?) + i64::from(mem.u32(SQ)?) + 2)?;
        mem.write8(chan + 4, i64::from(value))?;
        for j in 0..VOICE_COUNT {
            let mem = &mut self.s.mem;
            let v = voice_at(j);
            if u32::from(mem.u16(v + 4)?) == u32::from(mem.u8(slot)?) & 0xf
                && mem.u16(v + 0x1c)? == mem.u16(slot + 0x24)?
                && u32::from(mem.u16(v + 6)?) == i as u32
                && mem.u16(v + 8)? != 1
                && mem.u16(v)? == 1
            {
                mem.write16(v + 0x38, i64::from(value))?;
                let pan = 0.max(0x7f.min(s16(i64::from(mem.u16(v + 0x38)?) + i64::from(mem.u16(v + 0x3a)?) + 0xffc0)));
                mem.write16(v + 0x24, i64::from(mem.u8(PAN_TABLE + 2 * i64::from(pan >> 2))?))?;
                mem.write16(v + 0x26, i64::from(mem.u8(PAN_TABLE + 2 * i64::from(pan >> 2) + 1)?))?;
                let left = self.voice_volume(j, 0)?;
                self.voice_param(j, reg::VOLL, i64::from(left))?;
                let right = self.voice_volume(j, 1)?;
                self.voice_param(j, reg::VOLR, i64::from(right))?;
            }
        }
        let mem = &mut self.s.mem;
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)
    }

    fn controller_loop_count(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        if mem.u16(slot + 0x34)? == 0 {
            mem.write16(slot + 0x30, i64::from(mem.u8(slot + 3)?))?;
            mem.write16(slot + 0x34, 0)?;
        }
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)
    }

    fn controller_loop(&mut self, i: i32) -> Result<()> {
        let mem = &mut self.s.mem;
        let slot = slot_at(i);
        let value = u32::from(mem.u8(slot + 3)?);
        if value == 0x14 {
            mem.write16(slot + 0x2e, i64::from(mem.u8(slot)?))?;
            mem.write32(slot + 0x38, i64::from(mem.u32(slot + 0x14)?))?;
            mem.write16(slot + 0x32, 0)?;
            mem.write16(slot + 0x34, 0)?;
        } else if value == 0x1e {
            if mem.u16(slot + 0x30)? == 0x7f {
                mem.write16(slot + 0x2c, 1)?;
            } else if mem.u16(slot + 0x2a)? >= mem.u16(slot + 0x30)? {
                mem.write16(slot + 0x2a, 0)?;
                mem.write32(slot + 0x38, 0)?;
                mem.write16(slot + 0x2c, 0)?;
            } else {
                mem.write16(slot + 0x2a, i64::from(mem.u16(slot + 0x2a)?) + 1)?;
                mem.write16(slot + 0x2c, 1)?;
            }
            mem.write16(slot + 0x32, 0)?;
        }
        mem.write32(slot + 0x14, i64::from(mem.u32(slot + 0x14)?) + 3)
    }

    fn controller(&mut self, i: i32) -> Result<()> {
        let cc = u32::from(self.s.mem.u8(slot_at(i) + 2)?);
        match cc {
            7 => self.controller_volume(i),
            10 => self.controller_pan(i),
            6 => self.controller_loop_count(i),
            99 => self.controller_loop(i),
            _ => fail(format!("controller {cc} is not modelled (SNDCLOKS does not use it)")),
        }
    }

    fn release_finished(&mut self) -> Result<()> {
        for k in 0..VOICE_COUNT {
            let e = u32::from(self.s.feed_tick(SEQ_CORE as u32, k as u32)) & 0xffff;
            let mem = &mut self.s.mem;
            let v = voice_at(k);
            if (e & 0x7fff) < 3 && mem.u16(v + 8)? == 1 {
                let age = u32::from(mem.u16(v + 0xa)?);
                for j in 0..VOICE_COUNT {
                    let w = voice_at(j);
                    if age < u32::from(mem.u16(w + 0xa)?) && mem.u16(w + 0xa)? != 0xffff {
                        mem.write16(w + 0xa, i64::from(mem.u16(w + 0xa)?) - 1)?;
                    }
                }
                for b in 0..VOICE_SIZE {
                    mem.write8(v + b, 0)?;
                }
                mem.write16(v + 0x1c, 0xffff)?;
                mem.write16(v + 0xa, 0xffff)?;
                mem.write16(v + 6, 0xffff)?;
                let count = mem.u16(CTX + 0xc)?;
                mem.write16(CTX + 0xc, i64::from(count) - i64::from(count != 0))?;
            }
        }
        Ok(())
    }

    fn voice_updates(&self) -> Result<()> {
        let mem = &self.s.mem;
        for j in 0..VOICE_COUNT {
            let v = voice_at(j);
            if mem.u16(v)? == 1 && mem.u16(v + 0x14)? == 1 && mem.u16(v + 6)? < 0x18 {
                return fail("voice LFO update is not modelled (SNDCLOKS does not use it)");
            }
        }
        Ok(())
    }
}

pub fn voice_volume_of(mem: &IopMemory, j: i32, side: i32) -> Result<i32> {
    let v = VOICES + VOICE_SIZE * i64::from(j);
    let gain = u32::from(if side == 0 { mem.u16(v + 0x24)? } else { mem.u16(v + 0x26)? });
    let mut t = imul(i64::from(mem.u16(v + 0x2a)?), i64::from(mem.u16(v + 0x28)?));
    t = imul(i64::from(t), i64::from(mem.u16(v + 0x1e)?));
    t = imul(i64::from(t), i64::from(mem.u16(v + 0x20)?));
    let scaled = t >> 14;
    let pan = imul(i64::from(mem.u16(v + 0x22)?), i64::from(s16(i64::from(gain))));
    let mut r = (imul(i64::from(scaled), i64::from(pan)) >> 14) & 0xffff;
    if mem.u16(v + 0x36)? != 0 {
        r = (((s16(i64::from(r)) >> 7) as u32 | (u32::from(mem.u16(v + 0x36)?) << 8)) & 0xffff) as i32;
    }
    Ok(r)
}

impl State {
    pub fn tick(&mut self) -> Result<()> {
        Sequencer { s: self }.run()
    }
}
