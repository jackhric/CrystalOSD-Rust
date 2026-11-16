use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Boolean(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(Vec<u8>),
    Array(Vec<Json>),
    Object(BTreeMap<Vec<u8>, Json>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Uninitialized,
    LiteralTrue,
    LiteralFalse,
    LiteralNull,
    ValueString,
    ValueUnsigned,
    ValueInteger,
    ValueFloat,
    BeginArray,
    BeginObject,
    EndArray,
    EndObject,
    NameSeparator,
    ValueSeparator,
    ParseError,
    EndOfInput,
    LiteralOrValue,
}

fn token_name(t: Token) -> &'static str {
    match t {
        Token::Uninitialized => "<uninitialized>",
        Token::LiteralTrue => "true literal",
        Token::LiteralFalse => "false literal",
        Token::LiteralNull => "null literal",
        Token::ValueString => "string literal",
        Token::ValueUnsigned | Token::ValueInteger | Token::ValueFloat => "number literal",
        Token::BeginArray => "'['",
        Token::BeginObject => "'{'",
        Token::EndArray => "']'",
        Token::EndObject => "'}'",
        Token::NameSeparator => "':'",
        Token::ValueSeparator => "','",
        Token::ParseError => "<parse error>",
        Token::EndOfInput => "end of input",
        Token::LiteralOrValue => "'[', '{', or a literal",
    }
}

const EOF: i32 = -1;

const CONTROL: [&str; 32] = [
    "U+0000 (NUL) must be escaped to \\u0000",
    "U+0001 (SOH) must be escaped to \\u0001",
    "U+0002 (STX) must be escaped to \\u0002",
    "U+0003 (ETX) must be escaped to \\u0003",
    "U+0004 (EOT) must be escaped to \\u0004",
    "U+0005 (ENQ) must be escaped to \\u0005",
    "U+0006 (ACK) must be escaped to \\u0006",
    "U+0007 (BEL) must be escaped to \\u0007",
    "U+0008 (BS) must be escaped to \\u0008 or \\b",
    "U+0009 (HT) must be escaped to \\u0009 or \\t",
    "U+000A (LF) must be escaped to \\u000A or \\n",
    "U+000B (VT) must be escaped to \\u000B",
    "U+000C (FF) must be escaped to \\u000C or \\f",
    "U+000D (CR) must be escaped to \\u000D or \\r",
    "U+000E (SO) must be escaped to \\u000E",
    "U+000F (SI) must be escaped to \\u000F",
    "U+0010 (DLE) must be escaped to \\u0010",
    "U+0011 (DC1) must be escaped to \\u0011",
    "U+0012 (DC2) must be escaped to \\u0012",
    "U+0013 (DC3) must be escaped to \\u0013",
    "U+0014 (DC4) must be escaped to \\u0014",
    "U+0015 (NAK) must be escaped to \\u0015",
    "U+0016 (SYN) must be escaped to \\u0016",
    "U+0017 (ETB) must be escaped to \\u0017",
    "U+0018 (CAN) must be escaped to \\u0018",
    "U+0019 (EM) must be escaped to \\u0019",
    "U+001A (SUB) must be escaped to \\u001A",
    "U+001B (ESC) must be escaped to \\u001B",
    "U+001C (FS) must be escaped to \\u001C",
    "U+001D (GS) must be escaped to \\u001D",
    "U+001E (RS) must be escaped to \\u001E",
    "U+001F (US) must be escaped to \\u001F",
];

struct Lexer<'a> {
    input: &'a [u8],
    at: usize,
    current: i32,
    next_unget: bool,
    chars_read_total: usize,
    chars_read_current_line: usize,
    lines_read: usize,
    token_string: Vec<u8>,
    token_buffer: Vec<u8>,
    error_message: String,
    value_integer: i64,
    value_unsigned: u64,
    value_float: f64,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a [u8]) -> Self {
        Lexer {
            input,
            at: 0,
            current: EOF,
            next_unget: false,
            chars_read_total: 0,
            chars_read_current_line: 0,
            lines_read: 0,
            token_string: Vec::new(),
            token_buffer: Vec::new(),
            error_message: String::new(),
            value_integer: 0,
            value_unsigned: 0,
            value_float: 0.0,
        }
    }

    fn get(&mut self) -> i32 {
        self.chars_read_total += 1;
        self.chars_read_current_line += 1;
        if self.next_unget {
            self.next_unget = false;
        } else {
            self.current = match self.input.get(self.at) {
                Some(&b) => {
                    self.at += 1;
                    i32::from(b)
                }
                None => EOF,
            };
        }
        if self.current != EOF {
            self.token_string.push(self.current as u8);
        }
        if self.current == i32::from(b'\n') {
            self.lines_read += 1;
            self.chars_read_current_line = 0;
        }
        self.current
    }

    fn unget(&mut self) {
        self.next_unget = true;
        self.chars_read_total -= 1;
        if self.chars_read_current_line == 0 {
            if self.lines_read > 0 {
                self.lines_read -= 1;
            }
        } else {
            self.chars_read_current_line -= 1;
        }
        if self.current != EOF {
            self.token_string.pop();
        }
    }

    fn add(&mut self, c: i32) {
        self.token_buffer.push(c as u8);
    }

    fn reset(&mut self) {
        self.token_buffer.clear();
        self.token_string.clear();
        self.token_string.push(self.current as u8);
    }

    fn fail(&mut self, message: &str) -> Token {
        self.error_message = message.to_owned();
        Token::ParseError
    }

    fn get_codepoint(&mut self) -> i32 {
        let mut codepoint = 0;
        for factor in [12, 8, 4, 0] {
            let c = self.get();
            let digit = match c {
                0x30..=0x39 => c - 0x30,
                0x41..=0x46 => c - 0x37,
                0x61..=0x66 => c - 0x57,
                _ => return -1,
            };
            codepoint += digit << factor;
        }
        codepoint
    }

    fn next_byte_in_range(&mut self, ranges: &[(i32, i32)]) -> bool {
        self.add(self.current);
        for &(low, high) in ranges {
            let c = self.get();
            if low <= c && c <= high {
                self.add(c);
            } else {
                self.error_message = "invalid string: ill-formed UTF-8 byte".to_owned();
                return false;
            }
        }
        true
    }

    fn scan_string(&mut self) -> Token {
        self.reset();
        loop {
            let c = self.get();
            match c {
                EOF => return self.fail("invalid string: missing closing quote"),
                0x22 => return Token::ValueString,
                0x5C => match self.get() {
                    0x22 => self.add(0x22),
                    0x5C => self.add(0x5C),
                    0x2F => self.add(0x2F),
                    0x62 => self.add(0x08),
                    0x66 => self.add(0x0C),
                    0x6E => self.add(0x0A),
                    0x72 => self.add(0x0D),
                    0x74 => self.add(0x09),
                    0x75 => {
                        let codepoint1 = self.get_codepoint();
                        let mut codepoint = codepoint1;
                        if codepoint1 == -1 {
                            return self.fail("invalid string: '\\u' must be followed by 4 hex digits");
                        }
                        if (0xD800..=0xDBFF).contains(&codepoint1) {
                            if self.get() == 0x5C && self.get() == 0x75 {
                                let codepoint2 = self.get_codepoint();
                                if codepoint2 == -1 {
                                    return self.fail("invalid string: '\\u' must be followed by 4 hex digits");
                                }
                                if (0xDC00..=0xDFFF).contains(&codepoint2) {
                                    codepoint = (((codepoint1 as u32) << 10) + codepoint2 as u32 - 0x35FDC00) as i32;
                                } else {
                                    return self.fail("invalid string: surrogate U+D800..U+DBFF must be followed by U+DC00..U+DFFF");
                                }
                            } else {
                                return self.fail("invalid string: surrogate U+D800..U+DBFF must be followed by U+DC00..U+DFFF");
                            }
                        } else if (0xDC00..=0xDFFF).contains(&codepoint1) {
                            return self.fail("invalid string: surrogate U+DC00..U+DFFF must follow U+D800..U+DBFF");
                        }
                        let cp = codepoint as u32;
                        if cp < 0x80 {
                            self.add(cp as i32);
                        } else if cp <= 0x7FF {
                            self.add((0xC0 | (cp >> 6)) as i32);
                            self.add((0x80 | (cp & 0x3F)) as i32);
                        } else if cp <= 0xFFFF {
                            self.add((0xE0 | (cp >> 12)) as i32);
                            self.add((0x80 | ((cp >> 6) & 0x3F)) as i32);
                            self.add((0x80 | (cp & 0x3F)) as i32);
                        } else {
                            self.add((0xF0 | (cp >> 18)) as i32);
                            self.add((0x80 | ((cp >> 12) & 0x3F)) as i32);
                            self.add((0x80 | ((cp >> 6) & 0x3F)) as i32);
                            self.add((0x80 | (cp & 0x3F)) as i32);
                        }
                    }
                    _ => return self.fail("invalid string: forbidden character after backslash"),
                },
                0x00..=0x1F => return self.fail(&format!("invalid string: control character {}", CONTROL[c as usize])),
                0x20..=0x7F => self.add(c),
                0xC2..=0xDF => {
                    if !self.next_byte_in_range(&[(0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xE0 => {
                    if !self.next_byte_in_range(&[(0xA0, 0xBF), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xE1..=0xEC | 0xEE | 0xEF => {
                    if !self.next_byte_in_range(&[(0x80, 0xBF), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xED => {
                    if !self.next_byte_in_range(&[(0x80, 0x9F), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xF0 => {
                    if !self.next_byte_in_range(&[(0x90, 0xBF), (0x80, 0xBF), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xF1..=0xF3 => {
                    if !self.next_byte_in_range(&[(0x80, 0xBF), (0x80, 0xBF), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                0xF4 => {
                    if !self.next_byte_in_range(&[(0x80, 0x8F), (0x80, 0xBF), (0x80, 0xBF)]) {
                        return Token::ParseError;
                    }
                }
                _ => return self.fail("invalid string: ill-formed UTF-8 byte"),
            }
        }
    }

    fn scan_number(&mut self) -> Token {
        let digit = |c: i32| (0x30..=0x39).contains(&c);
        self.reset();
        let mut number = Token::ValueUnsigned;
        enum State {
            Minus,
            Zero,
            Any1,
            Decimal1,
            Decimal2,
            Exponent,
            Sign,
            Any2,
        }
        let mut state = match self.current {
            0x2D => State::Minus,
            0x30 => State::Zero,
            _ => State::Any1,
        };
        self.add(self.current);
        loop {
            state = match state {
                State::Minus => {
                    number = Token::ValueInteger;
                    match self.get() {
                        0x30 => State::Zero,
                        c if digit(c) => State::Any1,
                        _ => return self.fail("invalid number; expected digit after '-'"),
                    }
                }
                State::Zero => match self.get() {
                    0x2E => State::Decimal1,
                    0x65 | 0x45 => State::Exponent,
                    _ => break,
                },
                State::Any1 => match self.get() {
                    c if digit(c) => State::Any1,
                    0x2E => State::Decimal1,
                    0x65 | 0x45 => State::Exponent,
                    _ => break,
                },
                State::Decimal1 => {
                    number = Token::ValueFloat;
                    match self.get() {
                        c if digit(c) => State::Decimal2,
                        _ => return self.fail("invalid number; expected digit after '.'"),
                    }
                }
                State::Decimal2 => match self.get() {
                    c if digit(c) => State::Decimal2,
                    0x65 | 0x45 => State::Exponent,
                    _ => break,
                },
                State::Exponent => {
                    number = Token::ValueFloat;
                    match self.get() {
                        0x2B | 0x2D => State::Sign,
                        c if digit(c) => State::Any2,
                        _ => return self.fail("invalid number; expected '+', '-', or digit after exponent"),
                    }
                }
                State::Sign => match self.get() {
                    c if digit(c) => State::Any2,
                    _ => return self.fail("invalid number; expected digit after exponent sign"),
                },
                State::Any2 => match self.get() {
                    c if digit(c) => State::Any2,
                    _ => break,
                },
            };
            self.add(self.current);
        }
        self.unget();
        let text = std::str::from_utf8(&self.token_buffer).expect("ASCII digits");
        if number == Token::ValueUnsigned
            && let Ok(x) = text.parse::<u64>()
        {
            self.value_unsigned = x;
            return Token::ValueUnsigned;
        }
        if number == Token::ValueInteger
            && let Ok(x) = text.parse::<i64>()
        {
            self.value_integer = x;
            return Token::ValueInteger;
        }
        self.value_float = text.parse::<f64>().expect("a number the scanner accepted");
        Token::ValueFloat
    }

    fn scan_literal(&mut self, text: &[u8], token: Token) -> Token {
        for &expected in &text[1..] {
            if self.get() != i32::from(expected) {
                return self.fail("invalid literal");
            }
        }
        token
    }

    fn skip_bom(&mut self) -> bool {
        if self.get() == 0xEF {
            return self.get() == 0xBB && self.get() == 0xBF;
        }
        self.unget();
        true
    }

    fn skip_whitespace(&mut self) {
        loop {
            self.get();
            if !matches!(self.current, 0x20 | 0x09 | 0x0A | 0x0D) {
                break;
            }
        }
    }

    fn scan(&mut self) -> Token {
        if self.chars_read_total == 0 && !self.skip_bom() {
            return self.fail("invalid BOM; must be 0xEF 0xBB 0xBF if given");
        }
        self.skip_whitespace();
        match self.current {
            0x5B => Token::BeginArray,
            0x5D => Token::EndArray,
            0x7B => Token::BeginObject,
            0x7D => Token::EndObject,
            0x3A => Token::NameSeparator,
            0x2C => Token::ValueSeparator,
            0x74 => self.scan_literal(b"true", Token::LiteralTrue),
            0x66 => self.scan_literal(b"false", Token::LiteralFalse),
            0x6E => self.scan_literal(b"null", Token::LiteralNull),
            0x22 => self.scan_string(),
            0x2D | 0x30..=0x39 => self.scan_number(),
            0x00 | EOF => Token::EndOfInput,
            _ => self.fail("invalid literal"),
        }
    }

    fn token_string(&self) -> String {
        let mut out = String::new();
        for &c in &self.token_string {
            if c <= 0x1F {
                out += &format!("<U+{c:04X}>");
            } else {
                out.push(c as char);
            }
        }
        out
    }

    fn position(&self) -> String {
        format!(" at line {}, column {}", self.lines_read + 1, self.chars_read_current_line)
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    last: Token,
}

enum Frame {
    Array(Vec<Json>),
    Object(BTreeMap<Vec<u8>, Json>, Vec<u8>),
}

impl Parser<'_> {
    fn get_token(&mut self) -> Token {
        self.last = self.lexer.scan();
        self.last
    }

    fn syntax(&self, expected: Token, context: &str) -> String {
        let mut message = format!("syntax error while parsing {context} - ");
        if self.last == Token::ParseError {
            message += &format!("{}; last read: '{}'", self.lexer.error_message, self.lexer.token_string());
        } else {
            message += &format!("unexpected {}", token_name(self.last));
        }
        if expected != Token::Uninitialized {
            message += &format!("; expected {}", token_name(expected));
        }
        message
    }

    fn parse_error(&self, what: &str) -> String {
        format!("[json.exception.parse_error.101] parse error{}: {what}", self.lexer.position())
    }

    fn error(&self, expected: Token, context: &str) -> String {
        self.parse_error(&self.syntax(expected, context))
    }

    fn key(&self) -> Vec<u8> {
        self.lexer.token_buffer.clone()
    }

    fn parse(&mut self) -> Result<Json, String> {
        let mut stack: Vec<Frame> = Vec::new();
        self.get_token();
        loop {
            let mut value = match self.last {
                Token::BeginObject => {
                    if self.get_token() == Token::EndObject {
                        Some(Json::Object(BTreeMap::new()))
                    } else {
                        if self.last != Token::ValueString {
                            return Err(self.error(Token::ValueString, "object key"));
                        }
                        let key = self.key();
                        if self.get_token() != Token::NameSeparator {
                            return Err(self.error(Token::NameSeparator, "object separator"));
                        }
                        stack.push(Frame::Object(BTreeMap::new(), key));
                        self.get_token();
                        continue;
                    }
                }
                Token::BeginArray => {
                    if self.get_token() == Token::EndArray {
                        Some(Json::Array(Vec::new()))
                    } else {
                        stack.push(Frame::Array(Vec::new()));
                        continue;
                    }
                }
                Token::ValueFloat => {
                    let v = self.lexer.value_float;
                    if !v.is_finite() {
                        return Err(format!("[json.exception.out_of_range.406] number overflow parsing '{}'", self.lexer.token_string()));
                    }
                    Some(Json::Float(v))
                }
                Token::LiteralFalse => Some(Json::Boolean(false)),
                Token::LiteralNull => Some(Json::Null),
                Token::LiteralTrue => Some(Json::Boolean(true)),
                Token::ValueInteger => Some(Json::Integer(self.lexer.value_integer)),
                Token::ValueString => Some(Json::String(self.lexer.token_buffer.clone())),
                Token::ValueUnsigned => Some(Json::Unsigned(self.lexer.value_unsigned)),
                Token::ParseError => return Err(self.error(Token::Uninitialized, "value")),
                Token::EndOfInput => {
                    if self.lexer.chars_read_total == 1 {
                        return Err(self.parse_error("attempting to parse an empty input; check that your input string or stream contains the expected JSON"));
                    }
                    return Err(self.error(Token::LiteralOrValue, "value"));
                }
                _ => return Err(self.error(Token::LiteralOrValue, "value")),
            };
            loop {
                let Some(top) = stack.last_mut() else { return Ok(value.take().expect("a value")) };
                match top {
                    Frame::Array(items) => {
                        if let Some(v) = value.take() {
                            items.push(v);
                        }
                        if self.get_token() == Token::ValueSeparator {
                            self.get_token();
                            break;
                        }
                        if self.last == Token::EndArray {
                            let Some(Frame::Array(items)) = stack.pop() else { unreachable!() };
                            value = Some(Json::Array(items));
                            continue;
                        }
                        return Err(self.error(Token::EndArray, "array"));
                    }
                    Frame::Object(map, key) => {
                        if let Some(v) = value.take() {
                            map.insert(std::mem::take(key), v);
                        }
                        if self.get_token() == Token::ValueSeparator {
                            if self.get_token() != Token::ValueString {
                                return Err(self.error(Token::ValueString, "object key"));
                            }
                            let next = self.key();
                            if self.get_token() != Token::NameSeparator {
                                return Err(self.error(Token::NameSeparator, "object separator"));
                            }
                            if let Some(Frame::Object(_, key)) = stack.last_mut() {
                                *key = next;
                            }
                            self.get_token();
                            break;
                        }
                        if self.last == Token::EndObject {
                            let Some(Frame::Object(map, _)) = stack.pop() else { unreachable!() };
                            value = Some(Json::Object(map));
                            continue;
                        }
                        return Err(self.error(Token::EndObject, "object"));
                    }
                }
            }
        }
    }
}

pub fn parse(input: &[u8]) -> Result<Json, String> {
    let mut parser = Parser { lexer: Lexer::new(input), last: Token::Uninitialized };
    let value = parser.parse()?;
    if parser.get_token() != Token::EndOfInput {
        return Err(parser.error(Token::EndOfInput, "value"));
    }
    Ok(value)
}
