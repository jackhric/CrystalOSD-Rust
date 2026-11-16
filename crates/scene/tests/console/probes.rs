use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::inputs::{at, int, word};

// One probe record of a capture (tools/browser/export_browser_fixture.mjs probes.json): the program counter, the probe's index
// in the capture's list, the capture frame, the packet position, the registers and the memory it read.
#[derive(Debug, Clone)]
pub struct Record {
    pub pc: u32,
    pub index: Option<i64>,
    pub frame: i32,
    pub at: i64,
    pub gpr: Vec<u32>,
    pub mem: Vec<(u32, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub struct Write {
    pub frame: i32,
    pub address: u32,
    pub bytes: Vec<u8>,
}

pub struct Capture {
    pub name: String,
    pub rom: bool,
    pub records: Vec<Record>,
    pub writes: Vec<Write>,
}

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn load(path: &Path) -> Capture {
    let j = super::load_json(path);
    let records = at(&j, "probes")
        .as_array()
        .unwrap()
        .iter()
        .map(|r| Record {
            pc: word(at(r, "pc")),
            index: r.get("index").and_then(Value::as_i64),
            frame: int(at(r, "frame")),
            at: at(r, "at").as_i64().unwrap_or(-1),
            gpr: at(r, "gpr").as_array().unwrap().iter().map(word).collect(),
            mem: at(r, "mem").as_array().unwrap().iter().map(|m| (word(at(m, "address")), bytes(at(m, "hex").as_str().unwrap()))).collect(),
        })
        .collect();
    let writes = at(&j, "inputs")
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i.get("type").and_then(Value::as_str) == Some("write"))
        .map(|i| Write { frame: int(at(i, "captureFrame")), address: word(at(i, "address")), bytes: bytes(at(i, "bytes").as_str().unwrap_or("")) })
        .collect();
    Capture { name: at(&j, "capture").as_str().unwrap().to_owned(), rom: at(&j, "build").as_str() == Some("rom"), records, writes }
}

// The memory one or more records read, by address.
#[derive(Default, Clone)]
pub struct Image {
    parts: Vec<(u32, Vec<u8>)>,
}

impl Image {
    pub fn of<'r>(records: impl IntoIterator<Item = &'r Record>) -> Image {
        Image { parts: records.into_iter().flat_map(|r| r.mem.iter().cloned()).collect() }
    }

    pub fn has(&self, address: u32, size: u32) -> bool {
        self.parts.iter().any(|(a, b)| address >= *a && address + size <= a + b.len() as u32)
    }

    pub fn bytes(&self, address: u32, size: u32) -> &[u8] {
        for (a, b) in &self.parts {
            if address >= *a && address + size <= a + b.len() as u32 {
                let o = (address - a) as usize;
                return &b[o..o + size as usize];
            }
        }
        panic!("0x{address:08x} is not in the probe");
    }

    pub fn s32(&self, address: u32) -> i32 {
        i32::from_le_bytes(self.bytes(address, 4).try_into().unwrap())
    }

    pub fn u32(&self, address: u32) -> u32 {
        self.s32(address) as u32
    }

    pub fn u8(&self, address: u32) -> u8 {
        self.bytes(address, 1)[0]
    }

    pub fn f32(&self, address: u32) -> f32 {
        f32::from_le_bytes(self.bytes(address, 4).try_into().unwrap())
    }
}

impl Capture {
    // The records at `pc` in capture frame `frame`, in file order.
    pub fn at(&self, pc: u32, frame: i32) -> Vec<&Record> {
        self.records.iter().filter(|r| r.pc == pc && r.frame == frame).collect()
    }

    pub fn first(&self, pc: u32, frame: i32) -> Option<&Record> {
        self.records.iter().find(|r| r.pc == pc && r.frame == frame)
    }

    pub fn frames_of(&self, pc: u32) -> Vec<i32> {
        let mut frames: BTreeMap<i32, ()> = BTreeMap::new();
        for r in self.records.iter().filter(|r| r.pc == pc) {
            frames.insert(r.frame, ());
        }
        frames.into_keys().collect()
    }

    pub fn written(&self, frame: i32, address: u32) -> bool {
        self.writes.iter().any(|w| w.frame == frame && address >= w.address && address < w.address + w.bytes.len().max(4) as u32)
    }
}
