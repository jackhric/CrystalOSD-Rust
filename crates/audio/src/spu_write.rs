#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpuWrite {
    pub frame: u32,
    pub sample: u32,
    pub address: u32,
    pub value: u16,
    pub cycle: u32,
    pub resume: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriverCommand {
    pub frame: u32,
    pub sample: u64,
    pub id: u32,
    pub words: [u32; 5],
}

pub type WriteStream = Vec<SpuWrite>;

pub fn same_write(a: &SpuWrite, b: &SpuWrite) -> bool {
    a.frame == b.frame && a.address == b.address && a.value == b.value
}

pub fn first_difference(expected: &[SpuWrite], actual: &[SpuWrite]) -> Option<usize> {
    let n = expected.len().max(actual.len());
    (0..n).find(|&i| i >= expected.len() || i >= actual.len() || !same_write(&expected[i], &actual[i]))
}
