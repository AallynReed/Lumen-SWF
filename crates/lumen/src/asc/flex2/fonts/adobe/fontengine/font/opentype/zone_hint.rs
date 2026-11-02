//! Port of afe.jar `com.adobe.fontengine.font.opentype.ZoneHint`: blue zones and the standard stem of a TrueType font.

use std::cell::RefCell;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::auto_color::AutoColor;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::non_overlapping_outline_consumer::NonOverlappingOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_data::FontData;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontResult};
use crate::asc::util::jhash::JHashMap;

const K_ZONE_NONE: i32 = 12;
const K_BOT_ZONE_COLON: i32 = 4;
const K_TOP_ZONE_COLON: i32 = 11;
const K_TOP_ZONE_FIRST: i32 = 5;
const K_MAX_ZONE_HEIGHT: f64 = 36.0;

/// `ZoneTestChar`: code, top zone, bottom zone.
const G_ZONE_TEST_CHAR_LIST: [(u16, i32, i32); 65] = [
    (b'.' as u16, 12, 0), (b':' as u16, 11, 4), (b';' as u16, 11, 4), (b'0' as u16, 9, 2), (b'1' as u16, 10, 2), (b'2' as u16, 10, 2),
    (b'3' as u16, 9, 1), (b'4' as u16, 9, 1), (b'5' as u16, 9, 1), (b'6' as u16, 10, 2), (b'7' as u16, 9, 1), (b'8' as u16, 10, 2),
    (b'9' as u16, 9, 1), (b'A' as u16, 5, 0), (b'B' as u16, 5, 0), (b'C' as u16, 5, 0), (b'D' as u16, 5, 0), (b'E' as u16, 5, 0),
    (b'F' as u16, 5, 0), (b'G' as u16, 5, 0), (b'H' as u16, 5, 0), (b'I' as u16, 5, 0), (b'J' as u16, 5, 12), (b'K' as u16, 5, 0),
    (b'L' as u16, 5, 0), (b'M' as u16, 5, 0), (b'N' as u16, 5, 0), (b'O' as u16, 5, 0), (b'P' as u16, 5, 0), (b'Q' as u16, 5, 12),
    (b'R' as u16, 5, 0), (b'S' as u16, 5, 0), (b'T' as u16, 5, 0), (b'U' as u16, 5, 0), (b'V' as u16, 5, 0), (b'W' as u16, 5, 0),
    (b'X' as u16, 5, 0), (b'Y' as u16, 5, 0), (b'Z' as u16, 5, 0), (b'a' as u16, 7, 0), (b'b' as u16, 6, 0), (b'c' as u16, 7, 0),
    (b'd' as u16, 6, 0), (b'e' as u16, 7, 0), (b'f' as u16, 6, 0), (b'g' as u16, 7, 3), (b'h' as u16, 6, 0), (b'i' as u16, 8, 0),
    (b'j' as u16, 8, 3), (b'k' as u16, 6, 0), (b'l' as u16, 6, 0), (b'm' as u16, 7, 0), (b'n' as u16, 7, 0), (b'o' as u16, 7, 0),
    (b'p' as u16, 7, 3), (b'q' as u16, 7, 3), (b'r' as u16, 7, 0), (b's' as u16, 7, 0), (b't' as u16, 12, 0), (b'u' as u16, 7, 0),
    (b'v' as u16, 7, 0), (b'w' as u16, 7, 0), (b'x' as u16, 7, 0), (b'y' as u16, 7, 3), (b'z' as u16, 7, 0),
];

/// `ZoneHint.Zone`; the histogram is a Java `HashMap<Integer, Integer>`, whose order `getMode` depends on.
#[derive(Clone)]
struct Zone {
    zone_type: i32,
    num_values: i32,
    max_value: i32,
    min_value: i32,
    hist: JHashMap<i32, i32>,
}

impl Zone {
    fn new(zone_type: i32) -> Zone {
        Zone { zone_type, num_values: 0, max_value: 0, min_value: 0, hist: JHashMap::new() }
    }

    fn add_value(&mut self, n: i32) {
        self.add_value_n(n, 1);
    }

    fn add_value_n(&mut self, n: i32, n2: i32) {
        let n5 = self.hist.get(&n).cloned().unwrap_or(0);
        self.hist.insert(n, n5 + n2);
        self.set_min_max_with_value(n);
        self.num_values += n2;
    }

    fn remove_value(&mut self, n: i32) {
        if let Some(&n3) = self.hist.get(&n) {
            let n4 = n3 - 1;
            if n4 == 0 {
                self.hist.remove(&n);
                if n == self.min_value || n == self.max_value {
                    self.max_value = 0;
                    self.min_value = 0;
                    let keys: Vec<i32> = self.hist.keys().cloned().collect();
                    let mut first = true;
                    for k in keys {
                        if first {
                            self.min_value = k;
                            self.max_value = k;
                            first = false;
                        }
                        self.set_min_max_with_value(k);
                    }
                }
            } else {
                self.hist.insert(n, n4);
            }
            self.num_values -= 1;
        }
    }

    fn get_mode(&self) -> i32 {
        let mut n = 0;
        let mut n2 = 0;
        for (&k, &v) in self.hist.iter() {
            if v <= n2 {
                continue;
            }
            n = k;
            n2 = v;
        }
        if n2 == 1 {
            i32::MAX
        } else {
            n
        }
    }

    fn get_average(&self) -> f64 {
        let mut d = 0.0;
        for (&k, &v) in self.hist.iter() {
            d += k.wrapping_mul(v) as f64;
        }
        d / self.num_values as f64
    }

    fn set_min_max_with_value(&mut self, n: i32) {
        if self.num_values == 0 || n > self.max_value {
            self.max_value = n;
        }
        if self.num_values == 0 || n < self.min_value {
            self.min_value = n;
        }
    }

    fn get_height(&self) -> i32 {
        self.max_value.wrapping_sub(self.min_value)
    }
}

#[derive(Clone)]
pub struct ZoneHint {
    v_stems: Zone,
    top_zones: Vec<Zone>,
    bot_zones: Vec<Zone>,
    top_zone_was_set: bool,
    bot_zone_was_set: bool,
    char_top_extreme: i32,
    char_bot_extreme: i32,
    scaled_max_zone_height: i32,
    top_zone_data: Option<Vec<i32>>,
    bot_zone_data: Option<Vec<i32>>,
    blue_values: Option<Vec<i32>>,
    other_blues: Option<Vec<i32>>,
    blue_fuzz: i32,
    std_v_stem: f64,
    blue_scale: f64,
}

impl ZoneHint {
    pub fn new(font: &OpenTypeFont) -> FontResult<ZoneHint> {
        let d = font.get_units_per_em_x()?;
        let mut z = ZoneHint {
            v_stems: Zone::new(K_ZONE_NONE),
            top_zones: (0..7).map(|i| Zone::new(K_TOP_ZONE_FIRST + i)).collect(),
            bot_zones: (0..5).map(Zone::new).collect(),
            top_zone_was_set: false,
            bot_zone_was_set: false,
            char_top_extreme: 0,
            char_bot_extreme: 0,
            scaled_max_zone_height: (K_MAX_ZONE_HEIGHT * d / 1000.0) as i32,
            top_zone_data: None,
            bot_zone_data: None,
            blue_values: None,
            other_blues: None,
            blue_fuzz: 0,
            std_v_stem: 0.0,
            blue_scale: 0.0,
        };
        let auto_color = Rc::new(RefCell::new(AutoColor::new(None, d, 14, false, true, &[], &[])));
        let ac_dyn: Rc<RefCell<dyn OutlineConsumer>> = auto_color.clone();
        let mut non_overlapping = NonOverlappingOutlineConsumer::new(ac_dyn, d);
        for (n3, &(code, top, bot)) in G_ZONE_TEST_CHAR_LIST.iter().enumerate() {
            let n2 = font.get_glyph_for_char(code as i32)?;
            if n2 <= 0 {
                continue;
            }
            let r: FontResult<()> = (|| {
                z.char_bot_extreme = i32::MAX;
                z.char_top_extreme = i32::MAX;
                z.bot_zone_was_set = false;
                z.top_zone_was_set = false;
                // Java takes the advance of the loop index, not of the glyph.
                let d2 = font.get_horizontal_advance(n3 as i32)?;
                auto_color.borrow_mut().new_glyph(n2, d2, code as i32);
                font.get_glyph_outline_with(n2, &mut non_overlapping, 2)?;
                let a = auto_color.borrow().report_zones();
                let mut n4 = 0;
                while n4 < a.len() {
                    let n5 = a[n4];
                    n4 += 1;
                    match n5 {
                        0 => {
                            let (n6, n7) = (a[n4], a[n4 + 1]);
                            n4 += 2;
                            z.add_v_stem(n6, n7);
                        }
                        1 | 4 => n4 += 2,
                        2 => {
                            let (n8, n9) = (a[n4], a[n4 + 1]);
                            n4 += 2;
                            z.add_char_extreme(n8, n9);
                        }
                        3 => {
                            let (n8, n9) = (a[n4], a[n4 + 1]);
                            n4 += 2;
                            z.add_char_zone(n8, n9, top, bot);
                        }
                        _ => {}
                    }
                }
                if top < K_TOP_ZONE_COLON && !z.top_zone_was_set && z.char_top_extreme != i32::MAX {
                    let v = z.char_top_extreme;
                    z.top_zones[(top - 5) as usize].add_value(v);
                }
                if bot >= K_BOT_ZONE_COLON || z.bot_zone_was_set || z.char_bot_extreme == i32::MAX {
                    return Ok(());
                }
                let v = z.char_bot_extreme;
                z.bot_zones[bot as usize].add_value(v);
                Ok(())
            })();
            match r {
                Err(e) if e.kind == ExceptionKind::InvalidGlyph => {}
                r => r?,
            }
        }
        let top_sorted = z.sort_zones(true);
        let bot_sorted = z.sort_zones(false);
        if let Some(zi) = ZoneHint::find_zone(&bot_sorted, 0) {
            let zone = bot_sorted[zi].clone();
            let mut n10 = 0;
            let mut top_data = vec![0; top_sorted.len() * 2];
            for (n2, z2) in top_sorted.iter().enumerate() {
                if z2.get_height() > n10 {
                    n10 = z2.get_height();
                }
                top_data[n2 * 2] = z2.min_value;
                top_data[n2 * 2 + 1] = z2.max_value;
            }
            let mut bot_data = vec![0; bot_sorted.len() * 2];
            for (n2, z3) in bot_sorted.iter().enumerate() {
                if z3.get_height() > n10 {
                    n10 = z3.get_height();
                }
                bot_data[n2 * 2] = z3.min_value;
                bot_data[n2 * 2 + 1] = z3.max_value;
            }
            let mut blue = vec![zone.min_value, zone.max_value];
            blue.extend_from_slice(&top_data);
            let mut other = Vec::with_capacity(bot_data.len().saturating_sub(2));
            for (i, z4) in bot_sorted.iter().enumerate() {
                if i == zi {
                    continue;
                }
                other.push(z4.min_value);
                other.push(z4.max_value);
            }
            if n10 > 0 {
                z.blue_scale = 1.0 / n10 as f64 * 0.8;
            }
            z.blue_fuzz = 0;
            z.top_zone_data = Some(top_data);
            z.bot_zone_data = Some(bot_data);
            z.blue_values = Some(blue);
            z.other_blues = Some(other);
        }
        let mut n = z.v_stems.get_mode();
        if n == i32::MAX {
            n = (z.v_stems.get_average() + 0.5) as i32;
        }
        z.std_v_stem = n as f64;
        Ok(z)
    }

    /// `sortZones(mTopZones)` / `sortZones(mBotZones)`: merges and trims the zones; they are copies, as Java's are the
    /// same objects but never read again unsorted.
    fn sort_zones(&self, top: bool) -> Vec<Zone> {
        let zones = if top { &self.top_zones } else { &self.bot_zones };
        let mut list: Vec<Zone> = Vec::new();
        for zone in zones {
            if zone.num_values == 0 {
                continue;
            }
            let mut i = 0;
            while i <= list.len() {
                if i != list.len() && zone.min_value > list[i].min_value {
                    i += 1;
                    continue;
                }
                list.insert(i, zone.clone());
                break;
            }
        }
        let mut n = 0;
        while n < list.len() {
            let mut advance = true;
            if n + 1 < list.len() {
                let z2 = list[n + 1].clone();
                let zone = &list[n];
                if zone.max_value + 1 >= z2.min_value || zone.min_value + self.scaled_max_zone_height >= z2.max_value {
                    let zone = &mut list[n];
                    for (&k, &v) in z2.hist.iter() {
                        zone.add_value_n(k, v);
                    }
                    if zone.zone_type > z2.zone_type {
                        zone.zone_type = z2.zone_type;
                    }
                    list.remove(n + 1);
                    advance = false;
                }
            }
            let smzh = self.scaled_max_zone_height;
            {
                let zone = &mut list[n];
                let mut i = zone.num_values / 3;
                while zone.get_height() > smzh && i > 0 {
                    let d2 = zone.get_average();
                    let d = (zone.min_value + zone.max_value) as f64 / 2.0;
                    if d2 >= d {
                        let v = zone.min_value;
                        zone.remove_value(v);
                    } else {
                        let v = zone.max_value;
                        zone.remove_value(v);
                    }
                    i -= 1;
                }
            }
            if list[n].get_height() > smzh {
                list.remove(n);
                advance = false;
            }
            if advance {
                n += 1;
            }
        }
        list
    }

    fn find_zone(list: &[Zone], n: i32) -> Option<usize> {
        list.iter().position(|z| z.zone_type == n)
    }

    pub fn get_top_zones(&self) -> Vec<i32> {
        self.top_zone_data.clone().unwrap_or_default()
    }

    pub fn get_bottom_zones(&self) -> Vec<i32> {
        self.bot_zone_data.clone().unwrap_or_default()
    }

    pub fn get_blue_values(&self) -> Option<Vec<f64>> {
        match &self.blue_values {
            Some(v) if !v.is_empty() => Some(v.iter().map(|&x| x as f64).collect()),
            _ => None,
        }
    }

    pub fn get_other_blues(&self) -> Option<Vec<f64>> {
        match &self.other_blues {
            Some(v) if !v.is_empty() => Some(v.iter().map(|&x| x as f64).collect()),
            _ => None,
        }
    }

    pub fn get_blue_fuzz(&self) -> i32 {
        self.blue_fuzz
    }

    pub fn get_std_v_stem(&self) -> f64 {
        self.std_v_stem
    }

    pub fn get_blue_scale(&self) -> f64 {
        self.blue_scale
    }

    fn add_v_stem(&mut self, n: i32, n2: i32) {
        self.v_stems.add_value(n.wrapping_sub(n2));
    }

    fn add_char_extreme(&mut self, n: i32, n2: i32) {
        self.char_top_extreme = n;
        self.char_bot_extreme = n2;
    }

    fn add_char_zone(&mut self, n: i32, n2: i32, top: i32, bot: i32) {
        if top != K_ZONE_NONE && (self.char_top_extreme == i32::MAX || n.wrapping_add(self.scaled_max_zone_height) >= self.char_top_extreme) {
            self.top_zones[(top - 5) as usize].add_value(n);
            self.top_zone_was_set = true;
        }
        if bot != K_ZONE_NONE && (self.char_bot_extreme == i32::MAX || n2.wrapping_sub(self.scaled_max_zone_height) <= self.char_bot_extreme) {
            self.bot_zones[bot as usize].add_value(n2);
            self.bot_zone_was_set = true;
        }
    }
}
