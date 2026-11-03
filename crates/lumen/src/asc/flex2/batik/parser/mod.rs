//! Ports of Batik's `parser` package as the bridges use it: `AbstractParser`/`NumberParser`, `LengthParser`,
//! `PathParser` with `AWTPathProducer`, `PointsParser` with `AWTPolylineProducer`/`AWTPolygonProducer`,
//! `TransformListParser` with `AWTTransformProducer`, `PreserveAspectRatioParser` and `UnitProcessor`. The default
//! `ErrorHandler` rethrows, so the first error ends a parse.

use crate::asc::flex2::batik::ext::awt::geom::ExtendedGeneralPath;
use crate::asc::flex2::fonts::java2d::java::awt::geom::affine_transform::AffineTransform;
use crate::asc::flex2::fonts::java2d::java::awt::geom::shapes::Path2D;

/// `ParseException`, by its message.
#[derive(Debug, Clone)]
pub struct ParseException(pub String);

pub type PResult<T> = Result<T, ParseException>;

/// `StringNormalizingReader` with `AbstractParser.current`.
pub struct Reader {
    chars: Vec<char>,
    pos: usize,
    pub current: i32,
}

impl Reader {
    pub fn new(s: &str) -> Reader {
        let mut chars = Vec::new();
        let mut it = s.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\r' {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                chars.push('\n');
            } else {
                chars.push(c);
            }
        }
        Reader { chars, pos: 0, current: -1 }
    }

    pub fn read(&mut self) -> i32 {
        if self.pos >= self.chars.len() {
            return -1;
        }
        let c = self.chars[self.pos] as i32;
        self.pos += 1;
        c
    }

    fn advance(&mut self) {
        self.current = self.read();
    }

    fn err<T>(&self, key: &str, args: &[String]) -> PResult<T> {
        let m = match key {
            "character.expected" => format!("The character '{}' was expected. The current character code is: {}.", args[0], args[1]),
            "character.unexpected" => format!("Unexpected character (code: {}).", args[0]),
            "end.of.stream.expected" => format!("End of stream expected. Instead, a character with code {} has been encountered.", args[0]),
            _ => key.to_string(),
        };
        Err(ParseException(m))
    }

    fn unexpected<T>(&self) -> PResult<T> {
        self.err("character.unexpected", &[self.current.to_string()])
    }

    fn expect(&mut self, c: char) -> PResult<()> {
        self.advance();
        if self.current != c as i32 {
            return self.err("character.expected", &[c.to_string(), self.current.to_string()]);
        }
        Ok(())
    }

    pub fn skip_spaces(&mut self) {
        while matches!(self.current, 0x20 | 0x09 | 0x0D | 0x0A) {
            self.advance();
        }
    }

    pub fn skip_comma_spaces(&mut self) {
        self.skip_spaces();
        if self.current == ',' as i32 {
            self.advance();
            self.skip_spaces();
        }
    }

    fn is_digit(c: i32) -> bool {
        (b'0' as i32..=b'9' as i32).contains(&c)
    }

    /// `NumberParser.parseFloat()`.
    pub fn parse_float(&mut self) -> PResult<f32> {
        let mut mant: i32 = 0;
        let mut mant_dig = 0;
        let mut mant_pos = true;
        let mut mant_read = false;
        let mut exp: i32 = 0;
        let mut exp_dig = 0;
        let mut exp_adj: i32 = 0;
        let mut exp_pos = true;
        if self.current == '-' as i32 {
            mant_pos = false;
            self.advance();
        } else if self.current == '+' as i32 {
            self.advance();
        }
        let c = self.current;
        let mut skip_int = false;
        if c == '.' as i32 {
        } else if c == '0' as i32 {
            mant_read = true;
            loop {
                self.advance();
                let c = self.current;
                if (b'1' as i32..=b'9' as i32).contains(&c) {
                    break;
                }
                if c == '.' as i32 || c == 'e' as i32 || c == 'E' as i32 {
                    skip_int = true;
                    break;
                }
                if c != '0' as i32 {
                    return Ok(0.0);
                }
            }
        } else if !Self::is_digit(c) {
            return self.unexpected();
        }
        if !skip_int && Self::is_digit(self.current) {
            mant_read = true;
            loop {
                if mant_dig < 9 {
                    mant_dig += 1;
                    mant = mant * 10 + (self.current - '0' as i32);
                } else {
                    exp_adj += 1;
                }
                self.advance();
                if !Self::is_digit(self.current) {
                    break;
                }
            }
        }
        if self.current == '.' as i32 {
            self.advance();
            let c = self.current;
            'm2: {
                if !Self::is_digit(c) {
                    if !mant_read {
                        return self.unexpected();
                    }
                    break 'm2;
                }
                if c == '0' as i32 && mant_dig == 0 {
                    loop {
                        self.advance();
                        exp_adj -= 1;
                        let c = self.current;
                        if (b'1' as i32..=b'9' as i32).contains(&c) {
                            break;
                        }
                        if c != '0' as i32 {
                            if !mant_read {
                                return Ok(0.0);
                            }
                            break 'm2;
                        }
                    }
                }
                loop {
                    if mant_dig < 9 {
                        mant_dig += 1;
                        mant = mant * 10 + (self.current - '0' as i32);
                        exp_adj -= 1;
                    }
                    self.advance();
                    if !Self::is_digit(self.current) {
                        break;
                    }
                }
            }
        }
        if self.current == 'e' as i32 || self.current == 'E' as i32 {
            self.advance();
            let c = self.current;
            if c == '-' as i32 || c == '+' as i32 {
                if c == '-' as i32 {
                    exp_pos = false;
                }
                self.advance();
                if !Self::is_digit(self.current) {
                    return self.unexpected();
                }
            } else if !Self::is_digit(c) {
                return self.unexpected();
            }
            'en: {
                if self.current == '0' as i32 {
                    loop {
                        self.advance();
                        let c = self.current;
                        if (b'1' as i32..=b'9' as i32).contains(&c) {
                            break;
                        }
                        if c != '0' as i32 {
                            break 'en;
                        }
                    }
                }
                loop {
                    if exp_dig < 3 {
                        exp_dig += 1;
                        exp = exp * 10 + (self.current - '0' as i32);
                    }
                    self.advance();
                    if !Self::is_digit(self.current) {
                        break;
                    }
                }
            }
        }
        if !exp_pos {
            exp = -exp;
        }
        exp += exp_adj;
        if !mant_pos {
            mant = -mant;
        }
        Ok(build_float(mant, exp))
    }
}

/// `NumberParser.buildFloat(mant, exp)`.
pub fn build_float(mut mant: i32, exp: i32) -> f32 {
    if exp < -125 || mant == 0 {
        return 0.0;
    }
    if exp >= 128 {
        return if mant > 0 { f32::INFINITY } else { f32::NEG_INFINITY };
    }
    if exp == 0 {
        return mant as f32;
    }
    if mant >= (1 << 26) {
        mant += 1;
    }
    let pow10 = |i: i32| 10f64.powf(i as f64) as f32;
    if exp > 0 {
        mant as f32 * pow10(exp)
    } else {
        mant as f32 / pow10(-exp)
    }
}

pub const SVG_LENGTHTYPE_NUMBER: i16 = 1;
pub const SVG_LENGTHTYPE_PERCENTAGE: i16 = 2;
pub const SVG_LENGTHTYPE_EMS: i16 = 3;
pub const SVG_LENGTHTYPE_EXS: i16 = 4;
pub const SVG_LENGTHTYPE_PX: i16 = 5;
pub const SVG_LENGTHTYPE_CM: i16 = 6;
pub const SVG_LENGTHTYPE_MM: i16 = 7;
pub const SVG_LENGTHTYPE_IN: i16 = 8;
pub const SVG_LENGTHTYPE_PT: i16 = 9;
pub const SVG_LENGTHTYPE_PC: i16 = 10;

/// `LengthParser.parse(s)` with `UnitProcessor.UnitResolver`: the value and its unit.
pub fn parse_length(s: &str) -> PResult<(f32, i16)> {
    let mut r = Reader::new(s);
    r.advance();
    r.skip_spaces();
    let v = parse_length_value(&mut r)?;
    r.skip_spaces();
    if r.current != -1 {
        return r.err("end.of.stream.expected", &[r.current.to_string()]);
    }
    Ok(v)
}

fn parse_length_value(r: &mut Reader) -> PResult<(f32, i16)> {
    let mut mant: i32 = 0;
    let mut mant_dig = 0;
    let mut mant_pos = true;
    let mut mant_read = false;
    let mut exp: i32 = 0;
    let mut exp_dig = 0;
    let mut exp_adj: i32 = 0;
    let mut exp_pos = true;
    let mut unit_state = 0;
    let mut unit = SVG_LENGTHTYPE_NUMBER;
    if r.current == '-' as i32 {
        mant_pos = false;
        r.advance();
    } else if r.current == '+' as i32 {
        r.advance();
    }
    let c = r.current;
    let mut skip_int = false;
    if c == '.' as i32 {
    } else if c == '0' as i32 {
        mant_read = true;
        loop {
            r.advance();
            let c = r.current;
            if (b'1' as i32..=b'9' as i32).contains(&c) {
                break;
            }
            if c != '0' as i32 {
                skip_int = true;
                break;
            }
        }
    } else if !Reader::is_digit(c) {
        return r.unexpected();
    }
    if !skip_int && Reader::is_digit(r.current) {
        mant_read = true;
        loop {
            if mant_dig < 9 {
                mant_dig += 1;
                mant = mant * 10 + (r.current - '0' as i32);
            } else {
                exp_adj += 1;
            }
            r.advance();
            if !Reader::is_digit(r.current) {
                break;
            }
        }
    }
    if r.current == '.' as i32 {
        r.advance();
        let c = r.current;
        'm2: {
            if !Reader::is_digit(c) {
                if !mant_read {
                    return r.unexpected();
                }
                break 'm2;
            }
            if c == '0' as i32 && mant_dig == 0 {
                loop {
                    r.advance();
                    exp_adj -= 1;
                    let c = r.current;
                    if (b'1' as i32..=b'9' as i32).contains(&c) {
                        break;
                    }
                    if c != '0' as i32 {
                        break 'm2;
                    }
                }
            }
            loop {
                if mant_dig < 9 {
                    mant_dig += 1;
                    mant = mant * 10 + (r.current - '0' as i32);
                    exp_adj -= 1;
                }
                r.advance();
                if !Reader::is_digit(r.current) {
                    break;
                }
            }
        }
    }
    if r.current == 'e' as i32 || r.current == 'E' as i32 {
        let le = r.current == 'e' as i32;
        r.advance();
        let c = r.current;
        if c == 'm' as i32 || c == 'x' as i32 {
            if !le {
                return r.unexpected();
            }
            unit_state = if c == 'm' as i32 { 1 } else { 2 };
        } else {
            if c == '-' as i32 || c == '+' as i32 {
                if c == '-' as i32 {
                    exp_pos = false;
                }
                r.advance();
                if !Reader::is_digit(r.current) {
                    return r.unexpected();
                }
            } else if !Reader::is_digit(c) {
                return r.unexpected();
            }
            'en: {
                if r.current == '0' as i32 {
                    loop {
                        r.advance();
                        let c = r.current;
                        if (b'1' as i32..=b'9' as i32).contains(&c) {
                            break;
                        }
                        if c != '0' as i32 {
                            break 'en;
                        }
                    }
                }
                loop {
                    if exp_dig < 3 {
                        exp_dig += 1;
                        exp = exp * 10 + (r.current - '0' as i32);
                    }
                    r.advance();
                    if !Reader::is_digit(r.current) {
                        break;
                    }
                }
            }
        }
    }
    if !exp_pos {
        exp = -exp;
    }
    exp += exp_adj;
    if !mant_pos {
        mant = -mant;
    }
    let value = build_float(mant, exp);
    match unit_state {
        1 => {
            r.advance();
            return Ok((value, SVG_LENGTHTYPE_EMS));
        }
        2 => {
            r.advance();
            return Ok((value, SVG_LENGTHTYPE_EXS));
        }
        _ => {}
    }
    let two = |r: &mut Reader, opts: &[(char, i16)]| -> PResult<i16> {
        r.advance();
        for (c, u) in opts {
            if r.current == *c as i32 {
                r.advance();
                return Ok(*u);
            }
        }
        if opts.len() == 1 {
            return r.err("character.expected", &[opts[0].0.to_string(), r.current.to_string()]);
        }
        r.unexpected()
    };
    match char::from_u32(r.current as u32).unwrap_or('\0') {
        'e' => unit = two(r, &[('m', SVG_LENGTHTYPE_EMS), ('x', SVG_LENGTHTYPE_EXS)])?,
        'p' => unit = two(r, &[('c', SVG_LENGTHTYPE_PC), ('t', SVG_LENGTHTYPE_PT), ('x', SVG_LENGTHTYPE_PX)])?,
        'i' => unit = two(r, &[('n', SVG_LENGTHTYPE_IN)])?,
        'c' => unit = two(r, &[('m', SVG_LENGTHTYPE_CM)])?,
        'm' => unit = two(r, &[('m', SVG_LENGTHTYPE_MM)])?,
        '%' => {
            unit = SVG_LENGTHTYPE_PERCENTAGE;
            r.advance();
        }
        _ => {}
    }
    Ok((value, unit))
}

/// `PathParser.parse(s)` into an `AWTPathProducer`; on error the partial path is returned with it.
pub fn parse_path(s: &str, winding_rule: i32) -> (ExtendedGeneralPath, PResult<()>) {
    let mut p = AwtPathProducer { path: ExtendedGeneralPath::new(winding_rule), cur_x: 0.0, cur_y: 0.0, x_center: 0.0, y_center: 0.0 };
    let mut r = Reader::new(s);
    let res = p.do_parse(&mut r);
    (p.path, res)
}

struct AwtPathProducer {
    path: ExtendedGeneralPath,
    cur_x: f32,
    cur_y: f32,
    x_center: f32,
    y_center: f32,
}

fn is_num_start(c: i32) -> bool {
    c == '+' as i32 || c == '-' as i32 || c == '.' as i32 || Reader::is_digit(c)
}

impl AwtPathProducer {
    fn do_parse(&mut self, r: &mut Reader) -> PResult<()> {
        r.advance();
        loop {
            let c = r.current;
            match char::from_u32(c as u32).unwrap_or('\u{FFFF}') {
                '\r' | '\n' | ' ' | '\t' => r.advance(),
                'z' | 'Z' => {
                    r.advance();
                    self.path.close_path();
                    let (x, y) = self.path.get_current_point().unwrap_or((0.0, 0.0));
                    self.cur_x = x as f32;
                    self.cur_y = y as f32;
                }
                'm' | 'M' => {
                    let rel = c == 'm' as i32;
                    r.advance();
                    r.skip_spaces();
                    let x = r.parse_float()?;
                    r.skip_comma_spaces();
                    let y = r.parse_float()?;
                    if rel {
                        self.cur_x += x;
                        self.cur_y += y;
                    } else {
                        self.cur_x = x;
                        self.cur_y = y;
                    }
                    self.x_center = self.cur_x;
                    self.y_center = self.cur_y;
                    self.path.move_to(self.cur_x, self.cur_y);
                    r.skip_comma_spaces();
                    self.parse_l(r, rel)?;
                }
                'l' | 'L' => {
                    r.advance();
                    self.parse_l(r, c == 'l' as i32)?;
                }
                'h' | 'H' | 'v' | 'V' => {
                    r.advance();
                    r.skip_spaces();
                    while is_num_start(r.current) {
                        let v = r.parse_float()?;
                        match char::from_u32(c as u32).unwrap() {
                            'h' => self.cur_x += v,
                            'H' => self.cur_x = v,
                            'v' => self.cur_y += v,
                            _ => self.cur_y = v,
                        }
                        self.x_center = self.cur_x;
                        self.y_center = self.cur_y;
                        self.path.line_to(self.cur_x, self.cur_y);
                        r.skip_comma_spaces();
                    }
                }
                'c' | 'C' | 'q' | 'Q' | 's' | 'S' | 't' | 'T' | 'a' | 'A' => {
                    let cmd = char::from_u32(c as u32).unwrap();
                    r.advance();
                    r.skip_spaces();
                    let n = match cmd.to_ascii_lowercase() {
                        'c' => 6,
                        'q' | 's' => 4,
                        't' => 2,
                        _ => 7,
                    };
                    while is_num_start(r.current) {
                        let mut v = [0f32; 7];
                        let mut flags = [false; 2];
                        for i in 0..n {
                            if n == 7 && (i == 3 || i == 4) {
                                flags[i - 3] = match r.current {
                                    x if x == '0' as i32 => false,
                                    x if x == '1' as i32 => true,
                                    _ => return r.unexpected(),
                                };
                                r.advance();
                                r.skip_comma_spaces();
                                continue;
                            }
                            v[i] = r.parse_float()?;
                            if i + 1 < n {
                                r.skip_comma_spaces();
                            }
                        }
                        self.segment(cmd, &v, flags);
                        r.skip_comma_spaces();
                    }
                }
                _ if c == -1 => break,
                _ => return r.unexpected(),
            }
        }
        r.skip_spaces();
        if r.current != -1 {
            return r.err("end.of.stream.expected", &[r.current.to_string()]);
        }
        Ok(())
    }

    fn parse_l(&mut self, r: &mut Reader, rel: bool) -> PResult<()> {
        r.skip_spaces();
        while is_num_start(r.current) {
            let x = r.parse_float()?;
            r.skip_comma_spaces();
            let y = r.parse_float()?;
            if rel {
                self.cur_x += x;
                self.cur_y += y;
            } else {
                self.cur_x = x;
                self.cur_y = y;
            }
            self.x_center = self.cur_x;
            self.y_center = self.cur_y;
            self.path.line_to(self.cur_x, self.cur_y);
            r.skip_comma_spaces();
        }
        Ok(())
    }

    fn segment(&mut self, cmd: char, v: &[f32; 7], flags: [bool; 2]) {
        let (cx, cy) = (self.cur_x, self.cur_y);
        match cmd {
            'c' => {
                self.x_center = cx + v[2];
                self.y_center = cy + v[3];
                self.cur_x += v[4];
                self.cur_y += v[5];
                self.path.curve_to(cx + v[0], cy + v[1], self.x_center, self.y_center, self.cur_x, self.cur_y);
            }
            'C' => {
                self.x_center = v[2];
                self.y_center = v[3];
                self.cur_x = v[4];
                self.cur_y = v[5];
                self.path.curve_to(v[0], v[1], v[2], v[3], v[4], v[5]);
            }
            's' | 'S' => {
                let x1 = cx * 2.0 - self.x_center;
                let y1 = cy * 2.0 - self.y_center;
                if cmd == 's' {
                    self.x_center = cx + v[0];
                    self.y_center = cy + v[1];
                    self.cur_x += v[2];
                    self.cur_y += v[3];
                } else {
                    self.x_center = v[0];
                    self.y_center = v[1];
                    self.cur_x = v[2];
                    self.cur_y = v[3];
                }
                self.path.curve_to(x1, y1, self.x_center, self.y_center, self.cur_x, self.cur_y);
            }
            'q' => {
                self.x_center = cx + v[0];
                self.y_center = cy + v[1];
                self.cur_x += v[2];
                self.cur_y += v[3];
                self.path.quad_to(self.x_center, self.y_center, self.cur_x, self.cur_y);
            }
            'Q' => {
                self.x_center = v[0];
                self.y_center = v[1];
                self.cur_x = v[2];
                self.cur_y = v[3];
                self.path.quad_to(v[0], v[1], v[2], v[3]);
            }
            't' | 'T' => {
                self.x_center = cx * 2.0 - self.x_center;
                self.y_center = cy * 2.0 - self.y_center;
                if cmd == 't' {
                    self.cur_x += v[0];
                    self.cur_y += v[1];
                } else {
                    self.cur_x = v[0];
                    self.cur_y = v[1];
                }
                self.path.quad_to(self.x_center, self.y_center, self.cur_x, self.cur_y);
            }
            _ => {
                if cmd == 'a' {
                    self.cur_x += v[5];
                    self.cur_y += v[6];
                } else {
                    self.cur_x = v[5];
                    self.cur_y = v[6];
                }
                self.x_center = self.cur_x;
                self.y_center = self.cur_y;
                self.path.arc_to(v[0], v[1], v[2], flags[0], flags[1], self.cur_x, self.cur_y);
            }
        }
    }
}

/// `PointsParser.parse(s)` into an `AWTPolylineProducer` (or `AWTPolygonProducer` when `close`).
pub fn parse_points(s: &str, winding_rule: i32, close: bool) -> (Path2D, PResult<()>) {
    let mut path = Path2D::new_float(winding_rule);
    let mut r = Reader::new(s);
    let res = (|| {
        r.advance();
        r.skip_spaces();
        let mut new_path = true;
        while r.current != -1 {
            let x = r.parse_float()?;
            r.skip_comma_spaces();
            let y = r.parse_float()?;
            if new_path {
                new_path = false;
                path.move_to(x as f64, y as f64);
            } else {
                path.line_to(x as f64, y as f64);
            }
            r.skip_comma_spaces();
        }
        Ok(())
    })();
    if close && res.is_ok() {
        path.close_path();
    }
    (path, res)
}

/// `AffineTransform.getShearInstance(shx, shy)`.
fn shear_instance(shx: f64, shy: f64) -> AffineTransform {
    AffineTransform::from_values(1.0, shy, shx, 1.0, 0.0, 0.0)
}

/// `AWTTransformProducer.createAffineTransform(s)`.
pub fn parse_transform(s: &str) -> PResult<AffineTransform> {
    let mut at = AffineTransform::new();
    let mut r = Reader::new(s);
    loop {
        r.advance();
        let c = r.current;
        let open = |r: &mut Reader| -> PResult<()> {
            r.advance();
            r.skip_spaces();
            if r.current != '(' as i32 {
                return r.err("character.expected", &["(".into(), r.current.to_string()]);
            }
            r.advance();
            r.skip_spaces();
            Ok(())
        };
        let close = |r: &mut Reader| -> PResult<()> {
            if r.current != ')' as i32 {
                return r.err("character.expected", &[")".into(), r.current.to_string()]);
            }
            Ok(())
        };
        match char::from_u32(c as u32).unwrap_or('\u{FFFF}') {
            '\r' | '\n' | ' ' | '\t' | ',' => {}
            'm' => {
                for ch in "atrix".chars() {
                    r.expect(ch)?;
                }
                open(&mut r)?;
                let mut v = [0f32; 6];
                for (i, x) in v.iter_mut().enumerate() {
                    *x = r.parse_float()?;
                    if i < 5 {
                        r.skip_comma_spaces();
                    }
                }
                r.skip_spaces();
                close(&mut r)?;
                at.concatenate(&AffineTransform::from_values(v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64, v[4] as f64, v[5] as f64));
            }
            'r' => {
                for ch in "otate".chars() {
                    r.expect(ch)?;
                }
                open(&mut r)?;
                let theta = r.parse_float()?;
                r.skip_spaces();
                if r.current == ')' as i32 {
                    at.concatenate(&AffineTransform::get_rotate_instance(std::f64::consts::PI * theta as f64 / 180.0));
                    continue;
                }
                if r.current == ',' as i32 {
                    r.advance();
                    r.skip_spaces();
                }
                let cx = r.parse_float()?;
                r.skip_comma_spaces();
                let cy = r.parse_float()?;
                r.skip_spaces();
                close(&mut r)?;
                at.concatenate(&AffineTransform::get_rotate_instance_at(std::f64::consts::PI * theta as f64 / 180.0, cx as f64, cy as f64));
            }
            't' => {
                for ch in "ranslate".chars() {
                    r.expect(ch)?;
                }
                open(&mut r)?;
                let tx = r.parse_float()?;
                r.skip_spaces();
                if r.current == ')' as i32 {
                    at.concatenate(&AffineTransform::get_translate_instance(tx as f64, 0.0));
                    continue;
                }
                if r.current == ',' as i32 {
                    r.advance();
                    r.skip_spaces();
                }
                let ty = r.parse_float()?;
                r.skip_spaces();
                close(&mut r)?;
                at.concatenate(&AffineTransform::get_translate_instance(tx as f64, ty as f64));
            }
            's' => {
                r.advance();
                if r.current == 'c' as i32 {
                    for ch in "ale".chars() {
                        r.expect(ch)?;
                    }
                    open(&mut r)?;
                    let sx = r.parse_float()?;
                    r.skip_spaces();
                    if r.current == ')' as i32 {
                        at.concatenate(&AffineTransform::get_scale_instance(sx as f64, sx as f64));
                        continue;
                    }
                    if r.current == ',' as i32 {
                        r.advance();
                        r.skip_spaces();
                    }
                    let sy = r.parse_float()?;
                    r.skip_spaces();
                    close(&mut r)?;
                    at.concatenate(&AffineTransform::get_scale_instance(sx as f64, sy as f64));
                } else if r.current == 'k' as i32 {
                    for ch in "ew".chars() {
                        r.expect(ch)?;
                    }
                    r.advance();
                    let skew_x = match r.current {
                        x if x == 'X' as i32 => true,
                        x if x == 'Y' as i32 => false,
                        _ => return r.err("character.expected", &["X".into(), r.current.to_string()]),
                    };
                    open(&mut r)?;
                    let sk = r.parse_float()?;
                    r.skip_spaces();
                    close(&mut r)?;
                    let t = (std::f64::consts::PI * sk as f64 / 180.0).tan();
                    at.concatenate(&if skew_x { shear_instance(t, 0.0) } else { shear_instance(0.0, t) });
                } else {
                    return r.unexpected();
                }
            }
            _ if c == -1 => break,
            _ => return r.unexpected(),
        }
    }
    Ok(at)
}

pub const SVG_PRESERVEASPECTRATIO_UNKNOWN: i16 = 0;
pub const SVG_PRESERVEASPECTRATIO_NONE: i16 = 1;
pub const SVG_PRESERVEASPECTRATIO_XMINYMIN: i16 = 2;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMIN: i16 = 3;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMIN: i16 = 4;
pub const SVG_PRESERVEASPECTRATIO_XMINYMID: i16 = 5;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMID: i16 = 6;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMID: i16 = 7;
pub const SVG_PRESERVEASPECTRATIO_XMINYMAX: i16 = 8;
pub const SVG_PRESERVEASPECTRATIO_XMIDYMAX: i16 = 9;
pub const SVG_PRESERVEASPECTRATIO_XMAXYMAX: i16 = 10;

/// `PreserveAspectRatioParser.parse(s)` into a `ViewBox.ViewHandler`: `(align, meet)`.
pub fn parse_preserve_aspect_ratio(s: &str) -> PResult<(i16, bool)> {
    let mut r = Reader::new(s);
    r.advance();
    r.skip_spaces();
    let mut align = SVG_PRESERVEASPECTRATIO_UNKNOWN;
    let mut meet = true;
    let take = |r: &mut Reader, word: &str| -> PResult<()> {
        for ch in word.chars() {
            if r.current != ch as i32 {
                return r.err("character.expected", &[ch.to_string(), r.current.to_string()]);
            }
            r.advance();
        }
        Ok(())
    };
    if r.current == 'n' as i32 {
        take(&mut r, "none")?;
        r.skip_spaces();
        align = SVG_PRESERVEASPECTRATIO_NONE;
    } else if r.current == 'x' as i32 {
        take(&mut r, "xM")?;
        let x = if r.current == 'a' as i32 {
            take(&mut r, "ax")?;
            2
        } else if r.current == 'i' as i32 {
            r.advance();
            if r.current == 'd' as i32 {
                r.advance();
                1
            } else if r.current == 'n' as i32 {
                r.advance();
                0
            } else {
                return r.unexpected();
            }
        } else {
            return r.unexpected();
        };
        take(&mut r, "YM")?;
        let y = if r.current == 'a' as i32 {
            take(&mut r, "ax")?;
            2
        } else if r.current == 'i' as i32 {
            r.advance();
            if r.current == 'd' as i32 {
                r.advance();
                1
            } else if r.current == 'n' as i32 {
                r.advance();
                0
            } else {
                return r.unexpected();
            }
        } else {
            return r.unexpected();
        };
        align = SVG_PRESERVEASPECTRATIO_XMINYMIN + y * 3 + x;
    } else if r.current != -1 {
        return r.unexpected();
    }
    r.skip_comma_spaces();
    if r.current == 'm' as i32 {
        take(&mut r, "meet")?;
        meet = true;
    } else if r.current == 's' as i32 {
        take(&mut r, "slice")?;
        meet = false;
    } else if r.current != -1 {
        return r.unexpected();
    }
    r.skip_spaces();
    if r.current != -1 {
        return r.err("end.of.stream.expected", &[r.current.to_string()]);
    }
    Ok((align, meet))
}

/// `Float.parseFloat(s)`.
pub fn java_parse_float(s: &str) -> Option<f32> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    match t {
        "NaN" | "+NaN" | "-NaN" => return Some(f32::NAN),
        "Infinity" | "+Infinity" => return Some(f32::INFINITY),
        "-Infinity" => return Some(f32::NEG_INFINITY),
        _ => {}
    }
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) {
        return None;
    }
    t.parse::<f32>().ok()
}
