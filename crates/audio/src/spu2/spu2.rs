use super::core::{Core, ReverbRegs};
use super::reverb::ReverbPath;
use super::snapshot::{BASE, RAM_BYTES, REG_BYTES};
use super::voice::Voice;
use crate::{Result, fail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Voice,
    VoiceAddress,
    Core,
    Reverb,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Where {
    pub core: u8,
    pub kind: Kind,
    pub voice: u8,
    pub reg: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageSample {
    pub field: [i32; 24],
}

const WINDOW: u32 = 0x800;

fn offset_of(address: u32) -> Result<u32> {
    if !(BASE..BASE + REG_BYTES).contains(&address) {
        return fail("not an SPU2 register address");
    }
    Ok(address - BASE)
}

fn status_offset(offset: u32) -> bool {
    if (0x760..0x7B0).contains(&offset) {
        let i = (offset - 0x760) % 0x28 / 2;
        return i == 8 || i == 9;
    }
    if offset >= 0x800 {
        return false;
    }
    let o = offset & 0x3FF;
    if o < 0x180 {
        return (o % 16) / 2 >= 5;
    }
    if (0x1C0..0x2E0).contains(&o) {
        return ((o - 0x1C0) % 12) / 2 >= 4;
    }
    (0x1A0..0x1A8).contains(&o) || (0x340..0x346).contains(&o)
}

pub fn decode(address: u32) -> Result<Where> {
    let offset = offset_of(address)? & 0x7FF;
    if offset >= 0x760 {
        let core = u8::from((0x788..0x7B0).contains(&offset));
        return Ok(Where { core, kind: if offset < 0x7B0 { Kind::Reverb } else { Kind::Unknown }, voice: 0, reg: offset as u16 });
    }
    let core = u8::from(offset >= 0x400);
    let o = offset & 0x3FF;
    Ok(if o < 0x180 {
        Where { core, kind: Kind::Voice, voice: (o / 16) as u8, reg: (o % 16) as u16 }
    } else if (0x1C0..0x2E0).contains(&o) {
        Where { core, kind: Kind::VoiceAddress, voice: ((o - 0x1C0) / 12) as u8, reg: ((o - 0x1C0) % 12) as u16 }
    } else if (0x2E0..0x340).contains(&o) {
        Where { core, kind: Kind::Reverb, voice: 0, reg: o as u16 }
    } else if o < 0x346 {
        Where { core, kind: Kind::Core, voice: 0, reg: o as u16 }
    } else {
        Where { core, kind: Kind::Unknown, voice: 0, reg: o as u16 }
    })
}

pub fn is_status(address: u32) -> Result<bool> {
    Ok(status_offset(offset_of(address)? & 0x7FF))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    MasterLeftReg,
    MasterRightReg,
    FxVolLeft,
    FxVolRight,
    ExtVolLeft,
    ExtVolRight,
    InpVolLeft,
    InpVolRight,
    MasterLeftLevel,
    MasterRightLevel,
    IirVol,
    Comb1Vol,
    Comb2Vol,
    Comb3Vol,
    Comb4Vol,
    WallVol,
    Apf1Vol,
    Apf2Vol,
    InCoefL,
    InCoefR,
    VoiceLeftReg(usize),
    VoiceRightReg(usize),
    VoicePitch(usize),
    VoiceReg1(usize),
    VoiceReg2(usize),
    VoiceEnvelopeLevel(usize),
    VoiceLeftLevel(usize),
    VoiceRightLevel(usize),
    VoiceStart(usize),
    VoiceLoop(usize),
    VoiceNext(usize),
    Pmon,
    Non,
    Vmixl,
    Vmixel,
    Vmixr,
    Vmixer,
    Mmix,
    Attr,
    IrqAddress,
    KeyOn,
    KeyOff,
    TransferAddress,
    AutoDmaControl,
    EffectsStart,
    ReverbAddress(usize),
    EffectsEnd,
    Endx,
    Statx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Raw(usize),
    Spdif(usize),
    Field(usize, Field, u32),
}

fn reverb_address(r: &mut ReverbRegs, index: usize) -> &mut u32 {
    match index {
        0 => &mut r.apf1_size,
        1 => &mut r.apf2_size,
        2 => &mut r.same_l_dst,
        3 => &mut r.same_r_dst,
        4 => &mut r.comb1_l_src,
        5 => &mut r.comb1_r_src,
        6 => &mut r.comb2_l_src,
        7 => &mut r.comb2_r_src,
        8 => &mut r.same_l_src,
        9 => &mut r.same_r_src,
        10 => &mut r.diff_l_dst,
        11 => &mut r.diff_r_dst,
        12 => &mut r.comb3_l_src,
        13 => &mut r.comb3_r_src,
        14 => &mut r.comb4_l_src,
        15 => &mut r.comb4_r_src,
        16 => &mut r.diff_l_src,
        17 => &mut r.diff_r_src,
        18 => &mut r.apf1_l_dst,
        19 => &mut r.apf1_r_dst,
        20 => &mut r.apf2_l_dst,
        _ => &mut r.apf2_r_dst,
    }
}

fn locate(off: u32) -> Slot {
    if off >= WINDOW {
        return Slot::Raw((off & 0x1FFF) as usize);
    }
    if (0x760..0x7B0).contains(&off) {
        let c = usize::from(off >= 0x788);
        let field = match (off - 0x760 - if c == 1 { 0x28 } else { 0 }) / 2 {
            0 => Field::MasterLeftReg,
            1 => Field::MasterRightReg,
            2 => Field::FxVolLeft,
            3 => Field::FxVolRight,
            4 => Field::ExtVolLeft,
            5 => Field::ExtVolRight,
            6 => Field::InpVolLeft,
            7 => Field::InpVolRight,
            8 => Field::MasterLeftLevel,
            9 => Field::MasterRightLevel,
            10 => Field::IirVol,
            11 => Field::Comb1Vol,
            12 => Field::Comb2Vol,
            13 => Field::Comb3Vol,
            14 => Field::Comb4Vol,
            15 => Field::WallVol,
            16 => Field::Apf1Vol,
            17 => Field::Apf2Vol,
            18 => Field::InCoefL,
            _ => Field::InCoefR,
        };
        return Slot::Field(c, field, 0);
    }
    if (0x7C0..=0x7CC).contains(&off) {
        return Slot::Spdif(((off - 0x7C0) / 2) as usize);
    }
    if off >= 0x7B0 {
        return Slot::Raw(off as usize);
    }
    let c = usize::from(off >= 0x400);
    let o = off & 0x3FF;
    if o < 0x180 {
        let v = (o / 16) as usize;
        let field = match (o % 16) / 2 {
            0 => Field::VoiceLeftReg(v),
            1 => Field::VoiceRightReg(v),
            2 => Field::VoicePitch(v),
            3 => Field::VoiceReg1(v),
            4 => Field::VoiceReg2(v),
            5 => Field::VoiceEnvelopeLevel(v),
            6 => Field::VoiceLeftLevel(v),
            _ => Field::VoiceRightLevel(v),
        };
        return Slot::Field(c, field, 0);
    }
    let low_first = (o >> 1) & 1;
    let high_first = 1 - low_first;
    if o < 0x1C0 {
        let (field, half) = match o {
            0x180 | 0x182 => (Field::Pmon, low_first),
            0x184 | 0x186 => (Field::Non, low_first),
            0x188 | 0x18A => (Field::Vmixl, low_first),
            0x18C | 0x18E => (Field::Vmixel, low_first),
            0x190 | 0x192 => (Field::Vmixr, low_first),
            0x194 | 0x196 => (Field::Vmixer, low_first),
            0x198 => (Field::Mmix, 0),
            0x19A => (Field::Attr, 0),
            0x19C | 0x19E => (Field::IrqAddress, high_first),
            0x1A0 | 0x1A2 => (Field::KeyOn, low_first),
            0x1A4 | 0x1A6 => (Field::KeyOff, low_first),
            0x1A8 | 0x1AA => (Field::TransferAddress, high_first),
            0x1B0 => (Field::AutoDmaControl, 0),
            _ => return Slot::Raw(off as usize),
        };
        return Slot::Field(c, field, half);
    }
    if o < 0x2E0 {
        let v = ((o - 0x1C0) / 12) as usize;
        let (field, half) = match ((o - 0x1C0) % 12) / 2 {
            0 => (Field::VoiceStart(v), 1),
            1 => (Field::VoiceStart(v), 0),
            2 => (Field::VoiceLoop(v), 1),
            3 => (Field::VoiceLoop(v), 0),
            4 => (Field::VoiceNext(v), 1),
            _ => (Field::VoiceNext(v), 0),
        };
        return Slot::Field(c, field, half);
    }
    if o < 0x2E4 {
        return Slot::Field(c, Field::EffectsStart, high_first);
    }
    if o < 0x33C {
        return Slot::Field(c, Field::ReverbAddress(((o - 0x2E4) / 4) as usize), high_first);
    }
    match o {
        0x33C | 0x33E => Slot::Field(c, Field::EffectsEnd, high_first),
        0x340 => Slot::Field(c, Field::Endx, 0),
        0x342 => Slot::Field(c, Field::Endx, 1),
        0x344 => Slot::Field(c, Field::Statx, 0),
        _ => Slot::Raw(off as usize),
    }
}

#[derive(Clone, Copy)]
enum Gate {
    DryL,
    DryR,
    WetL,
    WetR,
}

fn gate_of(voice: &mut Voice, gate: Gate) -> &mut i32 {
    match gate {
        Gate::DryL => &mut voice.dry_l,
        Gate::DryR => &mut voice.dry_r,
        Gate::WetL => &mut voice.wet_l,
        Gate::WetR => &mut voice.wet_r,
    }
}

fn set_half(word: u32, half: u32, value: u16) -> u32 {
    let shift = 16 * half;
    (word & !(0xFFFF << shift)) | (u32::from(value) << shift)
}

fn with_half(word: &mut u32, half: u32, value: u16) {
    *word = set_half(*word, half, value);
}

fn with_half_signed(word: &mut i32, half: u32, value: u16) {
    *word = set_half(*word as u32, half, value) as i32;
}

pub(crate) fn write_ram_word(ram: &mut [u8], word_address: u32, value: u16) {
    let at = ((word_address & 0xFFFFF) * 2) as usize;
    ram[at] = value as u8;
    ram[at + 1] = (value >> 8) as u8;
}

#[derive(Debug, Clone)]
pub struct Spu2 {
    pub(crate) ram: Vec<u8>,
    pub(crate) raw: Vec<u8>,
    pub(crate) core: [Core; 2],
    pub(crate) spdif: [u16; 8],
    pub(crate) out_pos: u16,
    pub(crate) ticks: u32,
    pub(crate) play_mode: i32,
    pub(crate) reverb_path: ReverbPath,
    pub(crate) final_: (i16, i16),
}

impl Default for Spu2 {
    fn default() -> Self {
        Self::new()
    }
}

impl Spu2 {
    pub fn new() -> Self {
        let mut core = [Core::default(), Core::default()];
        for (i, c) in core.iter_mut().enumerate() {
            c.index = i as u32;
        }
        Self {
            ram: vec![0; RAM_BYTES as usize],
            raw: vec![0; REG_BYTES as usize],
            core,
            spdif: [0; 8],
            out_pos: 0,
            ticks: 0,
            play_mode: 0,
            reverb_path: ReverbPath::Avx,
            final_: (0, 0),
        }
    }

    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    pub fn core(&self, index: usize) -> &Core {
        &self.core[index]
    }

    pub fn voice(&self, core: usize, voice: usize) -> &Voice {
        &self.core[core].voices[voice]
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    pub fn set_ticks(&mut self, ticks: u32) {
        self.ticks = ticks;
    }

    pub fn set_reverb_path(&mut self, path: ReverbPath) {
        self.reverb_path = path;
    }

    fn field_bits(&self, c: usize, field: Field) -> u32 {
        let k = &self.core[c];
        let r = &k.reverb;
        match field {
            Field::MasterLeftReg => k.master_left.reg.into(),
            Field::MasterRightReg => k.master_right.reg.into(),
            Field::FxVolLeft => k.fx_vol.left as u32,
            Field::FxVolRight => k.fx_vol.right as u32,
            Field::ExtVolLeft => k.ext_vol.left as u32,
            Field::ExtVolRight => k.ext_vol.right as u32,
            Field::InpVolLeft => k.inp_vol.left as u32,
            Field::InpVolRight => k.inp_vol.right as u32,
            Field::MasterLeftLevel => k.master_left.level as u32,
            Field::MasterRightLevel => k.master_right.level as u32,
            Field::IirVol => (r.iir_vol as u16).into(),
            Field::Comb1Vol => (r.comb1_vol as u16).into(),
            Field::Comb2Vol => (r.comb2_vol as u16).into(),
            Field::Comb3Vol => (r.comb3_vol as u16).into(),
            Field::Comb4Vol => (r.comb4_vol as u16).into(),
            Field::WallVol => (r.wall_vol as u16).into(),
            Field::Apf1Vol => (r.apf1_vol as u16).into(),
            Field::Apf2Vol => (r.apf2_vol as u16).into(),
            Field::InCoefL => (r.in_coef_l as u16).into(),
            Field::InCoefR => (r.in_coef_r as u16).into(),
            Field::VoiceLeftReg(v) => k.voices[v].left.reg.into(),
            Field::VoiceRightReg(v) => k.voices[v].right.reg.into(),
            Field::VoicePitch(v) => k.voices[v].pitch.into(),
            Field::VoiceReg1(v) => k.voices[v].envelope.reg1.into(),
            Field::VoiceReg2(v) => k.voices[v].envelope.reg2.into(),
            Field::VoiceEnvelopeLevel(v) => k.voices[v].envelope.level as u32,
            Field::VoiceLeftLevel(v) => k.voices[v].left.level as u32,
            Field::VoiceRightLevel(v) => k.voices[v].right.level as u32,
            Field::VoiceStart(v) => k.voices[v].start_address,
            Field::VoiceLoop(v) => k.voices[v].loop_address,
            Field::VoiceNext(v) => k.voices[v].next_address,
            Field::Pmon => k.pmon,
            Field::Non => k.non,
            Field::Vmixl => k.vmixl,
            Field::Vmixel => k.vmixel,
            Field::Vmixr => k.vmixr,
            Field::Vmixer => k.vmixer,
            Field::Mmix => k.mmix.into(),
            Field::Attr => k.attr.into(),
            Field::IrqAddress => k.irq_address,
            Field::KeyOn => k.key_on,
            Field::KeyOff => k.key_off,
            Field::TransferAddress => k.transfer_address,
            Field::AutoDmaControl => k.auto_dma_control.into(),
            Field::EffectsStart => k.effects_start,
            Field::ReverbAddress(i) => {
                let mut copy = *r;
                *reverb_address(&mut copy, i)
            }
            Field::EffectsEnd => k.effects_end,
            Field::Endx => k.endx,
            Field::Statx => k.statx.into(),
        }
    }

    fn set_field(&mut self, c: usize, field: Field, half: u32, value: u16) {
        let k = &mut self.core[c];
        let r = &mut k.reverb;
        match field {
            Field::MasterLeftReg => k.master_left.reg = value,
            Field::MasterRightReg => k.master_right.reg = value,
            Field::FxVolLeft => with_half_signed(&mut k.fx_vol.left, half, value),
            Field::FxVolRight => with_half_signed(&mut k.fx_vol.right, half, value),
            Field::ExtVolLeft => with_half_signed(&mut k.ext_vol.left, half, value),
            Field::ExtVolRight => with_half_signed(&mut k.ext_vol.right, half, value),
            Field::InpVolLeft => with_half_signed(&mut k.inp_vol.left, half, value),
            Field::InpVolRight => with_half_signed(&mut k.inp_vol.right, half, value),
            Field::MasterLeftLevel => with_half_signed(&mut k.master_left.level, half, value),
            Field::MasterRightLevel => with_half_signed(&mut k.master_right.level, half, value),
            Field::IirVol => r.iir_vol = value as i16,
            Field::Comb1Vol => r.comb1_vol = value as i16,
            Field::Comb2Vol => r.comb2_vol = value as i16,
            Field::Comb3Vol => r.comb3_vol = value as i16,
            Field::Comb4Vol => r.comb4_vol = value as i16,
            Field::WallVol => r.wall_vol = value as i16,
            Field::Apf1Vol => r.apf1_vol = value as i16,
            Field::Apf2Vol => r.apf2_vol = value as i16,
            Field::InCoefL => r.in_coef_l = value as i16,
            Field::InCoefR => r.in_coef_r = value as i16,
            Field::VoiceLeftReg(v) => k.voices[v].left.reg = value,
            Field::VoiceRightReg(v) => k.voices[v].right.reg = value,
            Field::VoicePitch(v) => k.voices[v].pitch = value,
            Field::VoiceReg1(v) => k.voices[v].envelope.reg1 = value,
            Field::VoiceReg2(v) => k.voices[v].envelope.reg2 = value,
            Field::VoiceEnvelopeLevel(v) => with_half_signed(&mut k.voices[v].envelope.level, half, value),
            Field::VoiceLeftLevel(v) => with_half_signed(&mut k.voices[v].left.level, half, value),
            Field::VoiceRightLevel(v) => with_half_signed(&mut k.voices[v].right.level, half, value),
            Field::VoiceStart(v) => with_half(&mut k.voices[v].start_address, half, value),
            Field::VoiceLoop(v) => with_half(&mut k.voices[v].loop_address, half, value),
            Field::VoiceNext(v) => with_half(&mut k.voices[v].next_address, half, value),
            Field::Pmon => with_half(&mut k.pmon, half, value),
            Field::Non => with_half(&mut k.non, half, value),
            Field::Vmixl => with_half(&mut k.vmixl, half, value),
            Field::Vmixel => with_half(&mut k.vmixel, half, value),
            Field::Vmixr => with_half(&mut k.vmixr, half, value),
            Field::Vmixer => with_half(&mut k.vmixer, half, value),
            Field::Mmix => k.mmix = value,
            Field::Attr => k.attr = value,
            Field::IrqAddress => with_half(&mut k.irq_address, half, value),
            Field::KeyOn => with_half(&mut k.key_on, half, value),
            Field::KeyOff => with_half(&mut k.key_off, half, value),
            Field::TransferAddress => with_half(&mut k.transfer_address, half, value),
            Field::AutoDmaControl => k.auto_dma_control = value,
            Field::EffectsStart => with_half(&mut k.effects_start, half, value),
            Field::ReverbAddress(i) => with_half(reverb_address(r, i), half, value),
            Field::EffectsEnd => with_half(&mut k.effects_end, half, value),
            Field::Endx => with_half(&mut k.endx, half, value),
            Field::Statx => k.statx = value,
        }
    }

    fn get(&self, slot: Slot) -> u16 {
        match slot {
            Slot::Raw(i) => u16::from_le_bytes([self.raw[i], self.raw[i + 1]]),
            Slot::Spdif(i) => self.spdif[i],
            Slot::Field(c, field, half) => (self.field_bits(c, field) >> (16 * half)) as u16,
        }
    }

    fn set(&mut self, slot: Slot, value: u16) {
        match slot {
            Slot::Raw(i) => self.raw[i..i + 2].copy_from_slice(&value.to_le_bytes()),
            Slot::Spdif(i) => self.spdif[i] = value,
            Slot::Field(c, field, half) => self.set_field(c, field, half, value),
        }
    }

    pub fn read(&self, address: u32) -> Result<u16> {
        Ok(self.get(locate(offset_of(address)?)))
    }

    pub fn upload(&mut self, word_address: u32, words: &[u16]) {
        for (i, &w) in words.iter().enumerate() {
            write_ram_word(&mut self.ram, word_address.wrapping_add(i as u32), w);
        }
    }

    pub fn load_ram(&mut self, image: &[u8]) -> Result<()> {
        if image.len() != self.ram.len() {
            return fail("the RAM image is not the size of the SPU2 RAM");
        }
        self.ram.copy_from_slice(image);
        Ok(())
    }

    pub fn clear_ram(&mut self, byte_address: u32, bytes: u64) {
        let mut i = 0u64;
        while i < bytes && u64::from(byte_address) + i < self.ram.len() as u64 {
            self.ram[(u64::from(byte_address) + i) as usize] = 0;
            i += 1;
        }
    }

    fn data_port(&mut self, c: usize, value: u16) {
        let k = &mut self.core[c];
        k.active_transfer_address = k.transfer_address;
        write_ram_word(&mut self.ram, k.active_transfer_address, value);
        k.active_transfer_address = k.active_transfer_address.wrapping_add(1) & 0xFFFFF;
        k.transfer_address = k.active_transfer_address;
    }

    pub fn write(&mut self, address: u32, value: u16) -> Result<()> {
        self.write_register(offset_of(address)? & 0x7FF, value);
        Ok(())
    }

    fn set_gates(&mut self, c: usize, field: Field, gate: Gate, high: bool, value: u16) {
        let before = self.field_bits(c, field);
        self.set_field(c, field, u32::from(high), value);
        if before == self.field_bits(c, field) {
            return;
        }
        let (first, last) = if high { (16, 24) } else { (0, 16) };
        let mut bit = 1u32;
        for vc in first..last {
            *gate_of(&mut self.core[c].voices[vc], gate) = if u32::from(value) & bit != 0 { -1 } else { 0 };
            bit <<= 1;
        }
    }

    fn write_register(&mut self, off: u32, value: u16) {
        if (0x760..0x7B0).contains(&off) {
            let c = usize::from(off >= 0x788);
            let k = &mut self.core[c];
            match (off - 0x760 - if c == 1 { 0x28 } else { 0 }) / 2 {
                0 => k.master_left.write(value),
                1 => k.master_right.write(value),
                2 => k.fx_vol.left = i32::from(value as i16),
                3 => k.fx_vol.right = i32::from(value as i16),
                4 => k.ext_vol.left = i32::from(value as i16),
                5 => k.ext_vol.right = i32::from(value as i16),
                6 => k.inp_vol.left = i32::from(value as i16),
                7 => k.inp_vol.right = i32::from(value as i16),
                8 | 9 => {}
                _ => self.set(locate(off), value),
            }
            return;
        }
        if off >= 0x7B0 || (off & 0x3FF) >= 0x346 {
            self.set(locate(off), value);
            return;
        }
        let c = usize::from(off >= 0x400);
        let o = off & 0x3FF;
        if o < 0x180 {
            let v = &mut self.core[c].voices[(o / 16) as usize];
            match (o % 16) / 2 {
                0 => v.left.write(value),
                1 => v.right.write(value),
                2 => v.pitch = value,
                3 => v.envelope.reg1 = value,
                4 => v.envelope.reg2 = value,
                5 => self.set(locate(off), value),
                _ => {}
            }
            return;
        }
        if (0x1C0..0x2E0).contains(&o) {
            let v = &mut self.core[c].voices[((o - 0x1C0) / 12) as usize];
            let value = u32::from(value);
            match ((o - 0x1C0) % 12) / 2 {
                0 => v.start_address = ((value & 0x0F) << 16) | (v.start_address & 0xFFF8),
                1 => v.start_address = (v.start_address & 0x0F0000) | (value & 0xFFF8),
                2 => {
                    v.loop_mode = 1;
                    v.loop_address = ((value & 0x0F) << 16) | (v.loop_address & 0xFFF8);
                }
                3 => {
                    v.loop_mode = 1;
                    v.loop_address = (v.loop_address & 0x0F0000) | (value & 0xFFF8);
                }
                4 => v.next_address = ((value & 0x0F) << 16) | (v.next_address & 0xFFF8) | 1,
                _ => v.next_address = (v.next_address & 0x0F0000) | (value & 0xFFF8) | 1,
            }
            return;
        }
        match o {
            0x1AC => return self.data_port(c, value),
            0x19A => {
                let index = self.core[c].index;
                let k = &mut self.core[c];
                let irq_was = k.irq_enable;
                let dma_was = k.dma_mode;
                k.attr_bit0 = (value & 1) as u8;
                k.dma_bits = ((value >> 1) & 7) as i8;
                k.dma_mode = ((value >> 4) & 3) as u8;
                k.irq_enable = (value >> 6) & 1 != 0;
                k.fx_enable = (value >> 7) & 1 != 0;
                k.noise_clock = ((value >> 8) & 0x3F) as u8;
                k.mute = false;
                k.attr = value;
                if k.dma_mode == 0 && k.statx & 0x400 == 0 {
                    k.statx &= !0x80;
                } else if dma_was == 0 && k.dma_mode != 0 {
                    k.statx |= 0x80;
                }
                k.active_transfer_address = k.transfer_address;
                if irq_was != k.irq_enable && !k.irq_enable {
                    self.spdif[1] &= !((4u32 << index) as u16);
                }
                return;
            }
            0x180 => {
                let k = &mut self.core[c];
                for vc in 1..16 {
                    k.voices[vc].modulated = (value >> vc) & 1 != 0;
                }
                with_half(&mut k.pmon, 0, value);
                return;
            }
            0x182 => {
                let k = &mut self.core[c];
                for vc in 0..8 {
                    k.voices[vc + 16].modulated = (value >> vc) & 1 != 0;
                }
                with_half(&mut k.pmon, 1, value);
                return;
            }
            0x184 => {
                let k = &mut self.core[c];
                for vc in 0..16 {
                    k.voices[vc].noise = (value >> vc) & 1 != 0;
                }
                with_half(&mut k.non, 0, value);
                return;
            }
            0x186 => {
                let k = &mut self.core[c];
                for vc in 0..8 {
                    k.voices[vc + 16].noise = (value >> vc) & 1 != 0;
                }
                with_half(&mut k.non, 1, value);
                return;
            }
            0x188 => return self.set_gates(c, Field::Vmixl, Gate::DryL, false, value),
            0x18A => return self.set_gates(c, Field::Vmixl, Gate::DryL, true, value),
            0x18C => return self.set_gates(c, Field::Vmixel, Gate::WetL, false, value),
            0x18E => return self.set_gates(c, Field::Vmixel, Gate::WetL, true, value),
            0x190 => return self.set_gates(c, Field::Vmixr, Gate::DryR, false, value),
            0x192 => return self.set_gates(c, Field::Vmixr, Gate::DryR, true, value),
            0x194 => return self.set_gates(c, Field::Vmixer, Gate::WetR, false, value),
            0x196 => return self.set_gates(c, Field::Vmixer, Gate::WetR, true, value),
            0x198 => {
                let k = &mut self.core[c];
                let vx = value & if c == 0 { 0xFF0 } else { 0xFFF };
                let gate = |bit: u16| if vx & bit != 0 { -1 } else { 0 };
                k.wet_gate.ext_r = gate(0x001);
                k.wet_gate.ext_l = gate(0x002);
                k.dry_gate.ext_r = gate(0x004);
                k.dry_gate.ext_l = gate(0x008);
                k.wet_gate.inp_r = gate(0x010);
                k.wet_gate.inp_l = gate(0x020);
                k.dry_gate.inp_r = gate(0x040);
                k.dry_gate.inp_l = gate(0x080);
                k.wet_gate.snd_r = gate(0x100);
                k.wet_gate.snd_l = gate(0x200);
                k.dry_gate.snd_r = gate(0x400);
                k.dry_gate.snd_l = gate(0x800);
                k.mmix = value;
                return;
            }
            0x1B0 => {
                let k = &mut self.core[c];
                k.auto_dma_control = value;
                if value & 3 == 0 && k.auto_dma_active {
                    k.auto_dma_active = false;
                    let index = k.index;
                    for i in 0..0x200 {
                        write_ram_word(&mut self.ram, 0x2000 + (index << 10) + i, 0);
                        write_ram_word(&mut self.ram, 0x2200 + (index << 10) + i, 0);
                    }
                }
                return;
            }
            0x33C => {
                self.core[c].effects_end = ((u32::from(value) & 0xF) << 16) | 0xFFFF;
                return;
            }
            0x33E => return,
            0x324 | 0x326 if c == 1 => {
                with_half(&mut self.core[c].reverb.diff_r_src, 1 - ((o >> 1) & 1), value);
                return;
            }
            0x328 | 0x32A if c == 1 => {
                with_half(&mut self.core[c].reverb.diff_l_src, 1 - ((o >> 1) & 1), value);
                return;
            }
            _ => {}
        }
        self.set(locate(off), value);
    }

    pub fn advance(&mut self, samples: u32) {
        for _ in 0..samples {
            self.mix();
        }
    }

    pub fn final_left(&self) -> i16 {
        self.final_.0
    }

    pub fn final_right(&self) -> i16 {
        self.final_.1
    }

    pub fn render(&mut self, interleaved: &mut [i16], frames: usize) {
        for i in 0..frames {
            self.mix();
            interleaved[2 * i] = self.final_left();
            interleaved[2 * i + 1] = self.final_right();
        }
    }
}
