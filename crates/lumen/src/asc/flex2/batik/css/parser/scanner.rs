//! Port of `org.apache.flex.forks.batik.css.parser.Scanner` (Flex fork) and its `ParseException`.

use super::lexical_units as lu;
use super::scanner_utilities as su;
use crate::asc::flex2::batik::util::io::StreamNormalizingReader;

/// A `MessageFormat` argument.
#[derive(Clone, Debug)]
pub enum Param {
    Char(u16),
    Int(i32),
    Str(String),
}

/// `ParseException`: a message key with its arguments.
#[derive(Clone, Debug)]
pub struct ParseException {
    pub message: String,
    pub params: Option<Vec<Param>>,
    pub line_number: i32,
    pub column_number: i32,
}

impl ParseException {
    fn new(message: &str, params: Option<Vec<Param>>, line: i32, column: i32) -> ParseException {
        ParseException { message: message.to_string(), params, line_number: line, column_number: column }
    }
}

type PResult<T> = Result<T, ParseException>;

pub struct Scanner {
    reader: StreamNormalizingReader,
    current: i32,
    buffer: Vec<u16>,
    position: i32,
    type_: i32,
    start: i32,
    end: i32,
    blank_characters: i32,
}

fn ch(c: i32) -> u16 {
    c as u16
}

fn is_equal_ignore_case(i: i32, c: char) -> bool {
    i != -1 && java_to_lower(i as u16) == c as u16
}

/// `Character.toLowerCase(char)`.
pub fn java_to_lower(c: u16) -> u16 {
    if c == 0x130 {
        return 0x69;
    }
    match char::from_u32(c as u32) {
        Some(ch) => {
            let mut l = ch.to_lowercase();
            match (l.next(), l.next()) {
                (Some(x), None) if (x as u32) <= 0xFFFF => x as u16,
                _ => c,
            }
        }
        None => c,
    }
}

/// `Character.toUpperCase(char)`.
pub fn java_to_upper(c: u16) -> u16 {
    match char::from_u32(c as u32) {
        Some(ch) => {
            let mut u = ch.to_uppercase();
            match (u.next(), u.next()) {
                (Some(x), None) if (x as u32) <= 0xFFFF => x as u16,
                _ => c,
            }
        }
        None => c,
    }
}

/// `String.equalsIgnoreCase`.
pub trait JavaEqIc {
    fn java_eq_ic(&self, other: &str) -> bool;
}

impl JavaEqIc for str {
    fn java_eq_ic(&self, other: &str) -> bool {
        let a: Vec<u16> = crate::asc::util::jstr::units(self).collect();
        let b: Vec<u16> = crate::asc::util::jstr::units(other).collect();
        a.len() == b.len()
            && a.iter().zip(&b).all(|(&x, &y)| {
                x == y || {
                    let (ux, uy) = (java_to_upper(x), java_to_upper(y));
                    ux == uy || java_to_lower(ux) == java_to_lower(uy)
                }
            })
    }
}

impl Scanner {
    pub fn new(chars: Vec<u16>) -> Scanner {
        let mut s = Scanner { reader: StreamNormalizingReader::new(chars), current: 0, buffer: vec![0; 128], position: 0, type_: 0, start: 0, end: 0, blank_characters: 0 };
        s.current = s.next_char();
        s
    }

    pub fn get_line(&self) -> i32 {
        self.reader.get_line()
    }

    pub fn get_column(&self) -> i32 {
        self.reader.get_column()
    }

    pub fn get_start(&self) -> i32 {
        self.start
    }

    pub fn get_end(&self) -> i32 {
        self.end
    }

    pub fn clear_buffer(&mut self) {
        if self.position <= 0 {
            self.position = 0;
        } else {
            self.buffer[0] = self.buffer[(self.position - 1) as usize];
            self.position = 1;
        }
    }

    pub fn get_type(&self) -> i32 {
        self.type_
    }

    /// `new String(buffer, start, end - start)`, with its `StringIndexOutOfBoundsException` text.
    pub fn get_string_value(&self) -> Result<String, String> {
        let (offset, count, length) = (self.start, self.end - self.start, self.buffer.len() as i32);
        if offset < 0 || count < 0 || offset > length - count {
            return Err(format!("offset {offset}, count {count}, length {length}"));
        }
        Ok(crate::asc::util::jstr::from_utf16(&self.buffer[offset as usize..(offset + count) as usize]))
    }

    pub fn scan_at_rule(&mut self) {
        'outer: loop {
            match self.current {
                0x7B => {
                    let mut brackets = 1;
                    loop {
                        self.next_char();
                        match self.current {
                            0x7D => {
                                brackets -= 1;
                                if brackets > 0 {
                                    continue;
                                }
                                break 'outer;
                            }
                            -1 => break 'outer,
                            0x7B => brackets += 1,
                            _ => {}
                        }
                    }
                }
                -1 | 0x3B => break 'outer,
                _ => {}
            }
            self.next_char();
        }
        self.end = self.position;
    }

    pub fn next(&mut self) -> PResult<i32> {
        self.blank_characters = 0;
        self.start = self.position - 1;
        self.next_token()?;
        self.end = self.position - self.end_gap();
        Ok(self.type_)
    }

    fn end_gap(&self) -> i32 {
        let mut result = if self.current == -1 { 0 } else { 1 };
        match self.type_ {
            lu::FUNCTION | lu::STRING | lu::S | lu::PERCENTAGE => result += 1,
            lu::COMMENT | lu::HZ | lu::EM | lu::EX | lu::PC | lu::PT | lu::PX | lu::CM | lu::MM | lu::IN | lu::MS => result += 2,
            lu::KHZ | lu::DEG | lu::DPI | lu::RAD => result += 3,
            lu::DPCM | lu::GRAD => result += 4,
            _ => {}
        }
        result + self.blank_characters
    }

    fn err(&self, key: &str, params: Option<Vec<Param>>) -> ParseException {
        ParseException::new(key, params, self.reader.get_line(), self.reader.get_column())
    }

    fn err_char(&self, key: &str) -> ParseException {
        self.err(key, Some(vec![Param::Char(ch(self.current))]))
    }

    fn next_token(&mut self) -> PResult<()> {
        let c = self.current;
        let simple = |s: &mut Scanner, t: i32| {
            s.next_char();
            s.type_ = t;
        };
        match c {
            -1 => {
                self.type_ = lu::EOF;
                return Ok(());
            }
            0x7B => simple(self, lu::LEFT_CURLY_BRACE),
            0x7D => simple(self, lu::RIGHT_CURLY_BRACE),
            0x3D => simple(self, lu::EQUAL),
            0x2B => simple(self, lu::PLUS),
            0x2C => simple(self, lu::COMMA),
            0x3B => simple(self, lu::SEMI_COLON),
            0x3E => simple(self, lu::PRECEDE),
            0x5B => simple(self, lu::LEFT_BRACKET),
            0x5D => simple(self, lu::RIGHT_BRACKET),
            0x2A => simple(self, lu::ANY),
            0x28 => simple(self, lu::LEFT_BRACE),
            0x29 => simple(self, lu::RIGHT_BRACE),
            0x3A => simple(self, lu::COLON),
            0x20 | 0x09 | 0x0D | 0x0A | 0x0C => {
                loop {
                    self.next_char();
                    if !su::is_css_space(ch(self.current)) {
                        break;
                    }
                }
                self.type_ = lu::SPACE;
            }
            0x2F => {
                self.next_char();
                if self.current != '*' as i32 {
                    self.type_ = lu::DIVIDE;
                    return Ok(());
                }
                self.next_char();
                self.start = self.position - 1;
                loop {
                    while self.current != -1 && self.current != '*' as i32 {
                        self.next_char();
                    }
                    loop {
                        self.next_char();
                        if !(self.current != -1 && self.current == '*' as i32) {
                            break;
                        }
                    }
                    if !(self.current != -1 && self.current != '/' as i32) {
                        break;
                    }
                }
                if self.current == -1 {
                    return Err(self.err("eof", None));
                }
                self.next_char();
                self.type_ = lu::COMMENT;
            }
            0x27 => self.type_ = self.string1()?,
            0x22 => self.type_ = self.string2()?,
            0x3C => {
                self.next_char();
                if self.current != '!' as i32 {
                    return Err(self.err_char("character"));
                }
                self.next_char();
                if self.current == '-' as i32 {
                    self.next_char();
                    if self.current == '-' as i32 {
                        self.next_char();
                        self.type_ = lu::CDO;
                        return Ok(());
                    }
                }
                return Err(self.err_char("character"));
            }
            0x2D => {
                self.next_char();
                if self.identifier_or_function_checked()? {
                    return Ok(());
                }
                if self.current != '-' as i32 {
                    self.type_ = lu::MINUS;
                    return Ok(());
                }
                self.next_char();
                if self.current == '>' as i32 {
                    self.next_char();
                    self.type_ = lu::CDC;
                    return Ok(());
                }
                return Err(self.err_char("character"));
            }
            0x7C => {
                self.next_char();
                if self.current == '=' as i32 {
                    self.next_char();
                    self.type_ = lu::DASHMATCH;
                    return Ok(());
                } else if self.type_ == lu::IDENTIFIER {
                    self.type_ = lu::NAMESPACE_QUALIFIED;
                    return Ok(());
                }
                return Err(self.err_char("character"));
            }
            0x7E => {
                self.next_char();
                if self.current == '=' as i32 {
                    self.next_char();
                    self.type_ = lu::INCLUDES;
                    return Ok(());
                }
                return Err(self.err_char("character"));
            }
            0x23 => {
                self.next_char();
                if su::is_css_name_character(ch(self.current)) {
                    self.start = self.position - 1;
                    loop {
                        self.next_char();
                        if self.current == '\\' as i32 {
                            self.next_char();
                            self.escape()?;
                        }
                        if !(self.current != -1 && su::is_css_name_character(ch(self.current))) {
                            break;
                        }
                    }
                    self.type_ = lu::HASH;
                    return Ok(());
                }
                return Err(self.err_char("character"));
            }
            0x40 => {
                self.next_char();
                let keyword = |s: &mut Scanner, rest: &str, t: i32| -> bool {
                    s.start = s.position - 1;
                    for c in rest.chars() {
                        let n = s.next_char();
                        if !is_equal_ignore_case(n, c) {
                            return false;
                        }
                    }
                    s.next_char();
                    s.type_ = t;
                    true
                };
                let matched = match self.current {
                    0x63 | 0x43 => keyword(self, "harset", lu::CHARSET_SYMBOL),
                    0x66 | 0x46 => keyword(self, "ont-face", lu::FONT_FACE_SYMBOL),
                    0x69 | 0x49 => keyword(self, "mport", lu::IMPORT_SYMBOL),
                    0x6D | 0x4D => keyword(self, "edia", lu::MEDIA_SYMBOL),
                    0x6E | 0x4E => keyword(self, "amespace", lu::NAMESPACE),
                    0x70 | 0x50 => keyword(self, "age", lu::PAGE_SYMBOL),
                    _ => {
                        if !su::is_css_identifier_start_character(ch(self.current)) {
                            return Err(self.err_char("identifier.character"));
                        }
                        self.start = self.position - 1;
                        false
                    }
                };
                if matched {
                    return Ok(());
                }
                self.nm_char_sequence()?;
                self.type_ = lu::AT_KEYWORD;
            }
            0x21 => {
                loop {
                    self.next_char();
                    if !(self.current != -1 && su::is_css_space(ch(self.current))) {
                        break;
                    }
                }
                if is_equal_ignore_case(self.current, 'i') && "mportant".chars().all(|c| {
                    let n = self.next_char();
                    is_equal_ignore_case(n, c)
                }) {
                    self.next_char();
                    self.type_ = lu::IMPORTANT_SYMBOL;
                    return Ok(());
                }
                if self.current == -1 {
                    return Err(self.err("eof", None));
                } else {
                    return Err(self.err_char("character"));
                }
            }
            0x30..=0x39 => self.type_ = self.number()?,
            0x2E => match self.next_char() {
                0x30..=0x39 => self.type_ = self.dot_number(),
                _ => self.type_ = lu::DOT,
            },
            0x75 | 0x55 => {
                self.next_char();
                match self.current {
                    0x2B => {
                        let mut implied_range = false;
                        loop {
                            self.next_char();
                            if self.current == '?' as i32 {
                                implied_range = true;
                            }
                            if !(self.current != -1 && (self.current == '?' as i32 || su::is_css_hexadecimal_character(ch(self.current)))) {
                                break;
                            }
                        }
                        if self.current == '-' as i32 && !implied_range {
                            loop {
                                self.next_char();
                                if self.current == 'U' as i32 {
                                    self.next_char();
                                    if self.current == '+' as i32 {
                                        self.next_char();
                                    }
                                }
                                if !(self.current != -1 && su::is_css_hexadecimal_character(ch(self.current))) {
                                    break;
                                }
                            }
                        }
                        match self.current {
                            0x2C | 0x3B | 0x7D | 0x2F => {}
                            _ => {
                                if !su::is_css_space(ch(self.current)) {
                                    return Err(self.err_char("character"));
                                }
                            }
                        }
                        self.type_ = lu::UNICODE_RANGE;
                        return Ok(());
                    }
                    0x72 | 0x52 => {
                        self.next_char();
                        if let 0x6C | 0x4C = self.current {
                            self.next_char();
                            if self.current == '(' as i32 {
                                return self.url();
                            }
                        }
                    }
                    _ => {}
                }
                self.identifier_tail()?;
            }
            _ => {
                if !self.identifier_or_function_checked()? {
                    let tmp = self.current;
                    self.next_char();
                    return Err(self.err("identifier.character", Some(vec![Param::Char(ch(tmp))])));
                }
            }
        }
        Ok(())
    }

    /// The `url(` branch of `nextToken`.
    fn url(&mut self) -> PResult<()> {
        loop {
            self.next_char();
            if !(self.current != -1 && su::is_css_space(ch(self.current))) {
                break;
            }
        }
        match self.current {
            0x27 | 0x22 => {
                if self.current == 0x27 {
                    self.string1()?;
                } else {
                    self.string2()?;
                }
                self.blank_characters += 2;
                while self.current != -1 && su::is_css_space(ch(self.current)) {
                    self.blank_characters += 1;
                    self.next_char();
                }
                if self.current == -1 {
                    return Err(self.err("eof", None));
                }
                if self.current != ')' as i32 {
                    return Err(self.err_char("character"));
                }
                self.next_char();
                self.type_ = lu::URI;
                Ok(())
            }
            0x29 => Err(self.err_char("character")),
            _ => {
                if !su::is_css_uri_character(ch(self.current)) {
                    return Err(self.err_char("character"));
                }
                self.start = self.position - 1;
                loop {
                    self.next_char();
                    if !(self.current != -1 && su::is_css_uri_character(ch(self.current))) {
                        break;
                    }
                }
                self.blank_characters += 1;
                while self.current != -1 && su::is_css_space(ch(self.current)) {
                    self.blank_characters += 1;
                    self.next_char();
                }
                if self.current == -1 {
                    return Err(self.err("eof", None));
                }
                if self.current != ')' as i32 {
                    return Err(self.err_char("character"));
                }
                self.next_char();
                self.type_ = lu::URI;
                Ok(())
            }
        }
    }

    fn nm_char_sequence(&mut self) -> PResult<()> {
        loop {
            self.next_char();
            if self.current == '\\' as i32 {
                self.next_char();
                self.escape()?;
            }
            if !(self.current != -1 && su::is_css_name_character(ch(self.current))) {
                break;
            }
        }
        Ok(())
    }

    fn identifier_tail(&mut self) -> PResult<()> {
        if self.current == '(' as i32 {
            self.next_char();
            self.type_ = lu::FUNCTION;
        } else if self.current == -1 || !su::is_css_name_character(ch(self.current)) {
            self.type_ = lu::IDENTIFIER;
        } else {
            self.identifier_or_function_without_start_character_check()?;
        }
        Ok(())
    }

    fn identifier_or_function_without_start_character_check(&mut self) -> PResult<()> {
        self.nm_char_sequence()?;
        if self.current == '(' as i32 {
            self.next_char();
            self.type_ = lu::FUNCTION;
        } else {
            self.type_ = lu::IDENTIFIER;
        }
        Ok(())
    }

    fn identifier_or_function_checked(&mut self) -> PResult<bool> {
        if su::is_css_identifier_start_character(ch(self.current)) {
            self.identifier_or_function_without_start_character_check()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn string_n(&mut self, close: i32, other: i32) -> PResult<i32> {
        self.start = self.position;
        loop {
            let c = self.next_char();
            if c == -1 {
                return Err(self.err("eof", None));
            } else if c == close {
                break;
            } else if c == other {
            } else if c == '\\' as i32 {
                match self.next_char() {
                    0x0A | 0x0C => {}
                    _ => self.escape()?,
                }
            } else if !su::is_css_string_character(ch(self.current)) {
                return Err(self.err_char("character"));
            }
        }
        self.next_char();
        Ok(lu::STRING)
    }

    fn string1(&mut self) -> PResult<i32> {
        self.string_n('\'' as i32, '"' as i32)
    }

    fn string2(&mut self) -> PResult<i32> {
        self.string_n('"' as i32, '\'' as i32)
    }

    fn number(&mut self) -> PResult<i32> {
        loop {
            match self.next_char() {
                0x2E => match self.next_char() {
                    0x30..=0x39 => return Ok(self.dot_number()),
                    _ => return Err(self.err_char("character")),
                },
                0x30..=0x39 => {}
                _ => break,
            }
        }
        Ok(self.number_unit(true))
    }

    fn dot_number(&mut self) -> i32 {
        while let 0x30..=0x39 = self.next_char() {}
        self.number_unit(false)
    }

    fn name_chars(&self) -> bool {
        self.current != -1 && su::is_css_name_character(ch(self.current))
    }

    /// After a complete unit: the unit's type, or `DIMENSION` if more name characters follow.
    fn unit_end(&mut self, t: i32) -> i32 {
        self.next_char();
        if self.name_chars() {
            loop {
                self.next_char();
                if !self.name_chars() {
                    break;
                }
            }
            return lu::DIMENSION;
        }
        t
    }

    fn rest_dimension(&mut self) -> i32 {
        while self.name_chars() {
            self.next_char();
        }
        lu::DIMENSION
    }

    fn lower(c: i32) -> i32 {
        if (0x41..=0x5A).contains(&c) {
            c + 32
        } else {
            c
        }
    }

    fn number_unit(&mut self, integer: bool) -> i32 {
        match self.current {
            0x25 => {
                self.next_char();
                lu::PERCENTAGE
            }
            0x63 | 0x43 => match Self::lower(self.next_char()) {
                0x6D => self.unit_end(lu::CM),
                _ => self.rest_dimension(),
            },
            0x64 | 0x44 => {
                match Self::lower(self.next_char()) {
                    0x65 => {
                        if Self::lower(self.next_char()) == 0x67 {
                            return self.unit_end(lu::DEG);
                        }
                        // Java falls through into the `p` case.
                        self.dp()
                    }
                    0x70 => self.dp(),
                    _ => self.rest_dimension(),
                }
            }
            0x65 | 0x45 => match Self::lower(self.next_char()) {
                0x6D => self.unit_end(lu::EM),
                0x78 => self.unit_end(lu::EX),
                _ => self.rest_dimension(),
            },
            0x67 | 0x47 => {
                if Self::lower(self.next_char()) == 0x72 && Self::lower(self.next_char()) == 0x61 && Self::lower(self.next_char()) == 0x64 {
                    return self.unit_end(lu::GRAD);
                }
                self.rest_dimension()
            }
            0x68 | 0x48 => {
                self.next_char();
                match Self::lower(self.current) {
                    0x7A => self.unit_end(lu::HZ),
                    _ => self.rest_dimension(),
                }
            }
            0x69 | 0x49 => match Self::lower(self.next_char()) {
                0x6E => self.unit_end(lu::IN),
                _ => self.rest_dimension(),
            },
            0x6B | 0x4B => {
                if Self::lower(self.next_char()) == 0x68 && Self::lower(self.next_char()) == 0x7A {
                    return self.unit_end(lu::KHZ);
                }
                self.rest_dimension()
            }
            0x6D | 0x4D => match Self::lower(self.next_char()) {
                0x6D => self.unit_end(lu::MM),
                0x73 => self.unit_end(lu::MS),
                _ => self.rest_dimension(),
            },
            0x70 | 0x50 => match Self::lower(self.next_char()) {
                0x63 => self.unit_end(lu::PC),
                0x74 => self.unit_end(lu::PT),
                0x78 => self.unit_end(lu::PX),
                _ => self.rest_dimension(),
            },
            0x72 | 0x52 => {
                if Self::lower(self.next_char()) == 0x61 && Self::lower(self.next_char()) == 0x64 {
                    return self.unit_end(lu::RAD);
                }
                self.rest_dimension()
            }
            0x73 | 0x53 => {
                self.next_char();
                lu::S
            }
            _ => {
                if self.current != -1 && su::is_css_identifier_start_character(ch(self.current)) {
                    loop {
                        self.next_char();
                        if !self.name_chars() {
                            break;
                        }
                    }
                    return lu::DIMENSION;
                }
                if integer {
                    lu::INTEGER
                } else {
                    lu::REAL
                }
            }
        }
    }

    /// The `dp` prefix of `dpi` / `dpcm`, `current` being the `p`.
    fn dp(&mut self) -> i32 {
        match Self::lower(self.next_char()) {
            0x69 => self.unit_end(lu::DPI),
            0x63 => match Self::lower(self.next_char()) {
                0x6D => self.unit_end(lu::DPCM),
                _ => {
                    if self.name_chars() {
                        loop {
                            self.next_char();
                            if !self.name_chars() {
                                break;
                            }
                        }
                        return lu::DIMENSION;
                    }
                    // Java falls through to the default case.
                    self.rest_dimension()
                }
            },
            _ => self.rest_dimension(),
        }
    }

    fn escape(&mut self) -> PResult<()> {
        if su::is_css_hexadecimal_character(ch(self.current)) {
            for _ in 0..5 {
                self.next_char();
                if !su::is_css_hexadecimal_character(ch(self.current)) {
                    if su::is_css_space(ch(self.current)) {
                        self.next_char();
                    }
                    return Ok(());
                }
            }
        }
        if (self.current >= ' ' as i32 && self.current <= '~' as i32) || self.current >= 128 {
            self.next_char();
            return Ok(());
        }
        Err(self.err_char("character"))
    }

    fn next_char(&mut self) -> i32 {
        self.current = self.reader.read();
        if self.current == -1 {
            return self.current;
        }
        if self.position as usize == self.buffer.len() {
            let mut t = vec![0u16; (self.position * 3 / 2) as usize];
            t[..self.position as usize].copy_from_slice(&self.buffer[..self.position as usize]);
            self.buffer = t;
        }
        self.buffer[self.position as usize] = self.current as u16;
        self.position += 1;
        self.current
    }
}
