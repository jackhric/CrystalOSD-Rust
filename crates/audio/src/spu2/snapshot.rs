use std::path::Path;

use assets::bytes::read_file;

use crate::{Result, fail};

pub const BASE: u32 = 0x1F90_0000;
pub const RAM_BYTES: u32 = 0x200000;
pub const REG_BYTES: u32 = 0x10000;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(text: &str) -> Result<Json> {
        let mut parser = Parser { text: text.as_bytes(), at: 0 };
        let value = parser.value(0)?;
        parser.space();
        if parser.at != parser.text.len() {
            return fail("json: trailing characters");
        }
        Ok(value)
    }

    pub fn at(&self, key: &str) -> Result<&Json> {
        match self {
            Json::Object(members) => match members.iter().rev().find(|(k, _)| k == key) {
                Some((_, v)) => Ok(v),
                None => fail(format!("json: key '{key}' not found")),
            },
            _ => fail(format!("json: cannot use at() with a non-object for '{key}'")),
        }
    }

    pub fn index(&self, i: usize) -> Result<&Json> {
        match self {
            Json::Array(items) => match items.get(i) {
                Some(v) => Ok(v),
                None => fail(format!("json: array index {i} is out of range")),
            },
            _ => fail("json: cannot use [] with a non-array"),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Json::Null => 0,
            Json::Array(items) => items.len(),
            Json::Object(members) => members.len(),
            _ => 1,
        }
    }
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.at < self.text.len() && matches!(self.text[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn expect(&mut self, word: &[u8]) -> Result<()> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            fail("json: unexpected token")
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json> {
        if depth > 512 {
            return fail("json: nested too deeply");
        }
        self.space();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                self.space();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    self.space();
                    self.expect(b":")?;
                    let value = self.value(depth + 1)?;
                    members.push((key, value));
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Json::Object(members));
                        }
                        _ => return fail("json: expected ',' or '}'"),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                self.space();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.space();
                    match self.peek() {
                        Some(b',') => self.at += 1,
                        Some(b']') => {
                            self.at += 1;
                            return Ok(Json::Array(items));
                        }
                        _ => return fail("json: expected ',' or ']'"),
                    }
                }
            }
            Some(b'"') => Ok(Json::String(self.string()?)),
            Some(b't') => self.expect(b"true").map(|_| Json::Bool(true)),
            Some(b'f') => self.expect(b"false").map(|_| Json::Bool(false)),
            Some(b'n') => self.expect(b"null").map(|_| Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => fail("json: unexpected character"),
        }
    }

    fn string(&mut self) -> Result<String> {
        if self.peek() != Some(b'"') {
            return fail("json: expected a string");
        }
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return fail("json: unterminated string");
            };
            self.at += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return fail("json: unterminated string");
                    };
                    self.at += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hex =
                                self.text.get(self.at..self.at + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u32::from_str_radix(h, 16).ok());
                            let Some(code) = hex else {
                                return fail("json: bad \\u escape");
                            };
                            self.at += 4;
                            let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
                            out.extend_from_slice(ch.to_string().as_bytes());
                        }
                        _ => return fail("json: bad escape"),
                    }
                }
                _ => out.push(c),
            }
        }
        String::from_utf8(out).or_else(|_| fail("json: string is not UTF-8"))
    }

    fn number(&mut self) -> Result<Json> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let mut float = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => {}
                b'.' | b'e' | b'E' | b'+' | b'-' => float = true,
                _ => break,
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.text[start..self.at]).unwrap_or("");
        if !float {
            if let Ok(v) = text.parse::<i64>() {
                return Ok(Json::Integer(v));
            }
            if let Ok(v) = text.parse::<u64>() {
                return Ok(Json::Unsigned(v));
            }
        }
        match text.parse::<f64>() {
            Ok(v) => Ok(Json::Float(v)),
            Err(_) => fail("json: bad number"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub ram: Vec<u8>,
    pub regs: Vec<u8>,
    pub state: Json,
}

fn read_existing(path: &Path) -> Result<Vec<u8>> {
    if !path.exists() {
        return fail(format!("missing capture file {}", path.display()));
    }
    read_file(path)
}

impl Snapshot {
    pub fn load(captures: &Path, name: &str, which: &str) -> Result<Snapshot> {
        let stem = format!("{name}.{which}");
        let ram = read_existing(&captures.join(format!("{stem}.spuram.bin")))?;
        let regs = read_existing(&captures.join(format!("{stem}.spuregs.bin")))?;
        if ram.len() != RAM_BYTES as usize {
            return fail(format!("{stem}.spuram.bin: not 2 MB"));
        }
        if regs.len() != REG_BYTES as usize {
            return fail(format!("{stem}.spuregs.bin: not 64 KB"));
        }
        let state_path = captures.join(format!("{stem}.spustate.json"));
        if !state_path.exists() {
            return fail(format!("missing capture file {}", state_path.display()));
        }
        let text = std::fs::read_to_string(&state_path).or_else(|_| fail(format!("cannot open {}", state_path.display())))?;
        Ok(Snapshot { ram, regs, state: Json::parse(&text)? })
    }
}
