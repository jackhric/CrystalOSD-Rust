use super::envelope::VolumeSweep;
use super::voice::Voice;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReverbRegs {
    pub in_coef_l: i16,
    pub in_coef_r: i16,
    pub apf1_size: u32,
    pub apf2_size: u32,
    pub apf1_vol: i16,
    pub apf2_vol: i16,
    pub same_l_src: u32,
    pub same_r_src: u32,
    pub diff_l_src: u32,
    pub diff_r_src: u32,
    pub same_l_dst: u32,
    pub same_r_dst: u32,
    pub diff_l_dst: u32,
    pub diff_r_dst: u32,
    pub iir_vol: i16,
    pub wall_vol: i16,
    pub comb1_l_src: u32,
    pub comb1_r_src: u32,
    pub comb2_l_src: u32,
    pub comb2_r_src: u32,
    pub comb3_l_src: u32,
    pub comb3_r_src: u32,
    pub comb4_l_src: u32,
    pub comb4_r_src: u32,
    pub comb1_vol: i16,
    pub comb2_vol: i16,
    pub comb3_vol: i16,
    pub comb4_vol: i16,
    pub apf1_l_dst: u32,
    pub apf1_r_dst: u32,
    pub apf2_l_dst: u32,
    pub apf2_r_dst: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreGates {
    pub inp_l: i32,
    pub inp_r: i32,
    pub snd_l: i32,
    pub snd_r: i32,
    pub ext_l: i32,
    pub ext_r: i32,
}

impl Default for CoreGates {
    fn default() -> Self {
        Self { inp_l: -1, inp_r: -1, snd_l: -1, snd_r: -1, ext_l: 0, ext_r: 0 }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StereoVolume {
    pub left: i32,
    pub right: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Core {
    pub index: u32,
    pub voices: [Voice; 24],
    pub dry_gate: CoreGates,
    pub wet_gate: CoreGates,
    pub master_left: VolumeSweep,
    pub master_right: VolumeSweep,
    pub ext_vol: StereoVolume,
    pub inp_vol: StereoVolume,
    pub fx_vol: StereoVolume,
    pub irq_address: u32,
    pub transfer_address: u32,
    pub active_transfer_address: u32,
    pub irq_enable: bool,
    pub fx_enable: bool,
    pub mute: bool,
    pub auto_dma_active: bool,
    pub dma_bits: i8,
    pub noise_clock: u8,
    pub noise_counter: u32,
    pub noise_out: u32,
    pub auto_dma_control: u16,
    pub dma_mode: u8,
    pub attr_bit0: u8,
    pub pmon: u32,
    pub non: u32,
    pub vmixl: u32,
    pub vmixr: u32,
    pub vmixel: u32,
    pub vmixer: u32,
    pub endx: u32,
    pub mmix: u16,
    pub statx: u16,
    pub attr: u16,
    pub key_on: u32,
    pub key_off: u32,
    pub effects_start: u32,
    pub effects_end: u32,
    pub reverb_position: u32,
    pub reverb: ReverbRegs,
    pub down: [[i16; 128]; 2],
    pub up: [[i16; 128]; 2],
}

impl Default for Core {
    fn default() -> Self {
        Self {
            index: 0,
            voices: [Voice::default(); 24],
            dry_gate: CoreGates::default(),
            wet_gate: CoreGates::default(),
            master_left: VolumeSweep::default(),
            master_right: VolumeSweep::default(),
            ext_vol: StereoVolume { left: 0x7FFF, right: 0x7FFF },
            inp_vol: StereoVolume { left: 0x7FFF, right: 0x7FFF },
            fx_vol: StereoVolume::default(),
            irq_address: 0x800,
            transfer_address: 0,
            active_transfer_address: 0,
            irq_enable: false,
            fx_enable: false,
            mute: false,
            auto_dma_active: false,
            dma_bits: 0,
            noise_clock: 0,
            noise_counter: 0,
            noise_out: 0,
            auto_dma_control: 0,
            dma_mode: 0,
            attr_bit0: 0,
            pmon: 0,
            non: 0,
            vmixl: 0xFFFFFF,
            vmixr: 0xFFFFFF,
            vmixel: 0xFFFFFF,
            vmixer: 0xFFFFFF,
            endx: 0xFFFFFF,
            mmix: 0,
            statx: 0,
            attr: 0,
            key_on: 0,
            key_off: 0,
            effects_start: 0,
            effects_end: 0xFFFFF,
            reverb_position: 0,
            reverb: ReverbRegs::default(),
            down: [[0; 128]; 2],
            up: [[0; 128]; 2],
        }
    }
}
