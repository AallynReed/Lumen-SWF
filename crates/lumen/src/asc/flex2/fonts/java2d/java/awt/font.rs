//! Port of JDK 17 `java.awt.Font` (the parts JREFontManager reaches). A `Font` is shared like a Java reference:
//! clones see the same lazily found `Font2D` handle.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, Font2DHandle};
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::{SunFontManager, LOGICAL_FALLBACK};
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::FontFormatException;

pub const PLAIN: i32 = 0;
pub const BOLD: i32 = 1;
pub const ITALIC: i32 = 2;
pub const DIALOG: &str = "Dialog";

struct FontData {
    name: String,
    style: i32,
    size: i32,
    point_size: f32,
    created_font: bool,
    font2d_handle: RefCell<Option<Rc<Font2DHandle>>>,
}

#[derive(Clone)]
pub struct Font(Rc<FontData>);

impl Font {
    fn make(name: String, style: i32, size: i32, point_size: f32, created_font: bool, handle: Option<Rc<Font2DHandle>>) -> Font {
        Font(Rc::new(FontData { name, style, size, point_size, created_font, font2d_handle: RefCell::new(handle) }))
    }

    /// `new Font(name, style, size)`.
    pub fn new(name: Option<&str>, style: i32, size: i32) -> Font {
        let name = name.unwrap_or("Default").to_string();
        let style = if (style & !0x03) == 0 { style } else { 0 };
        Font::make(name, style, size, size as f32, false, None)
    }

    /// `new Font(Font2D)`, as `createFont` builds it.
    fn from_font2d(font2d: &Rc<Font2D>) -> Font {
        Font::make(font2d.get_font_name(Some(&Locale::get_default())), PLAIN, 1, 1.0, true, Some(font2d.handle()))
    }

    pub fn get_font2d(&self) -> Rc<Font2D> {
        if self.0.font2d_handle.borrow().is_none() {
            let fm = SunFontManager::get_instance();
            let f = fm.find_font2d(&self.0.name, self.0.style, LOGICAL_FALLBACK).expect("logical fallback");
            *self.0.font2d_handle.borrow_mut() = Some(f.handle());
        }
        self.0.font2d_handle.borrow().as_ref().unwrap().get()
    }

    /// `Font.createFont(Font.TRUETYPE_FONT, file)`.
    pub fn create_font(path: &str) -> Result<Font, FontFormatException> {
        let fm = SunFontManager::get_instance();
        let fonts = fm.create_font2d(path, false)?;
        Ok(Font::from_font2d(&fonts[0]))
    }

    /// `deriveFont(attributes)` with `getAttributes()` plus FAMILY, SIZE, POSTURE (oblique or regular) and WEIGHT
    /// (bold or regular) put, as JREFontManager does.
    pub fn derive_font(&self, family: &str, size: f32, oblique: bool, bold: bool) -> Font {
        let (weight, posture) = (if bold { 2.0f32 } else { 1.0 }, if oblique { 0.2f32 } else { 0.0 });
        let old_name = &self.0.name;
        let old_style = self.0.style;
        let mut created = self.0.created_font;
        let mut handle = if created { self.0.font2d_handle.borrow().clone() } else { None };
        if created {
            let new_name = if family == old_name { None } else { Some(family) };
            let mut new_style = 0;
            if weight >= 2.0 {
                new_style = BOLD;
            }
            if posture >= 0.2 {
                new_style |= ITALIC;
            }
            if old_style == new_style {
                new_style = -1;
            }
            let h = handle.clone().unwrap();
            if h.get().composite().is_some() {
                if new_style != -1 || new_name.is_some() {
                    handle = Some(SunFontManager::get_instance().get_new_composite(new_name, new_style, &h));
                }
            } else if new_name.is_some() {
                created = false;
                handle = None;
            }
        }
        let mut style = 0;
        if weight >= 2.0 {
            style |= BOLD;
        }
        if posture >= 0.2 {
            style |= ITALIC;
        }
        Font::make(family.to_string(), style, (size + 0.5) as i32, size, created, handle)
    }

    /// `Font.decode(str)`.
    pub fn decode(s: &str) -> Font {
        use crate::asc::util::jstr::{char_at, last_index_of_unit, substring, utf16_len};
        let mut font_size = 12;
        let mut font_style = PLAIN;
        let last_hyphen = last_index_of_unit(s, '-' as u16);
        let last_space = last_index_of_unit(s, ' ' as u16);
        let sep_char = if last_hyphen > last_space { '-' as u16 } else { ' ' as u16 };
        let mut size_index = last_index_of_unit(s, sep_char);
        let strlen = utf16_len(s);
        let mut style_index = if size_index >= 1 { last_index_of_from(s, sep_char, size_index - 1) } else { -1 };
        if size_index > 0 && size_index + 1 < strlen {
            match parse_java_int(&substring(s, size_index + 1, strlen)) {
                Some(v) => {
                    font_size = v;
                    if font_size <= 0 {
                        font_size = 12;
                    }
                }
                None => {
                    style_index = size_index;
                    size_index = strlen;
                    if char_at(s, size_index - 1) == sep_char {
                        size_index -= 1;
                    }
                }
            }
        }
        let font_name;
        if style_index >= 0 && style_index + 1 < strlen {
            let style_name = substring(s, style_index + 1, size_index).to_lowercase();
            match style_name.as_str() {
                "bolditalic" => font_style = BOLD | ITALIC,
                "italic" => font_style = ITALIC,
                "bold" => font_style = BOLD,
                "plain" => font_style = PLAIN,
                _ => {
                    style_index = size_index;
                    if char_at(s, style_index - 1) == sep_char {
                        style_index -= 1;
                    }
                }
            }
            font_name = substring(s, 0, style_index);
        } else {
            let mut font_end = strlen;
            if style_index > 0 {
                font_end = style_index;
            } else if size_index > 0 {
                font_end = size_index;
            }
            if font_end > 0 && char_at(s, font_end - 1) == sep_char {
                font_end -= 1;
            }
            font_name = substring(s, 0, font_end);
        }
        Font::new(Some(&font_name), font_style, font_size)
    }

    pub fn get_family(&self) -> String {
        self.get_font2d().get_family_name(Some(&Locale::get_default()))
    }

    pub fn get_ps_name(&self) -> String {
        self.get_font2d().get_postscript_name()
    }

    pub fn get_name(&self) -> String {
        self.0.name.clone()
    }

    pub fn get_font_name(&self, l: &Locale) -> String {
        self.get_font2d().get_font_name(Some(l))
    }

    pub fn get_style(&self) -> i32 {
        self.0.style
    }

    pub fn get_size(&self) -> i32 {
        self.0.size
    }

    pub fn get_size2d(&self) -> f32 {
        self.0.point_size
    }

    pub fn is_transformed(&self) -> bool {
        false
    }

    pub fn get_num_glyphs(&self) -> i32 {
        self.get_font2d().get_num_glyphs()
    }

    pub fn get_missing_glyph_code(&self) -> i32 {
        self.get_font2d().get_missing_glyph_code()
    }

    pub fn can_display(&self, c: u16) -> bool {
        self.get_font2d().can_display(c)
    }
}

/// `String.lastIndexOf(ch, fromIndex)`.
fn last_index_of_from(s: &str, ch: u16, from: i32) -> i32 {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut i = from.min(units.len() as i32 - 1);
    while i >= 0 {
        if units[i as usize] == ch {
            return i;
        }
        i -= 1;
    }
    -1
}

/// `Integer.valueOf(s)`: optional sign, ASCII or Unicode decimal digits, no overflow.
fn parse_java_int(s: &str) -> Option<i32> {
    let (neg, digits) = match s.chars().next()? {
        '-' => (true, &s[1..]),
        '+' => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty() {
        return None;
    }
    let mut v: i64 = 0;
    for c in digits.chars() {
        let d = c.to_digit(10)? as i64;
        v = v * 10 + d;
        if v > 2147483648 {
            return None;
        }
    }
    let v = if neg { -v } else { v };
    if v > i32::MAX as i64 || v < i32::MIN as i64 {
        return None;
    }
    Some(v as i32)
}
