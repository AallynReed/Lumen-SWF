//! Port of JDK 17 `sun.awt.FontConfiguration` as the Windows JDK runs it (default locale en_US, encoding
//! windows-1252): its `PropertiesHandler` tables hold the same values here, kept as maps instead of the short arrays
//! of the binary format, read from the JDK's `lib/fontconfig.properties.src` (the source of the `fontconfig.bfc` it loads).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::asc::util::jhash::JHashSet;

pub const NUM_FONTS: usize = 5;
pub const NUM_STYLES: usize = 4;
pub const FONT_NAMES: [&str; 5] = ["serif", "sansserif", "monospaced", "dialog", "dialoginput"];
pub const PUBLIC_FONT_NAMES: [&str; 5] = ["Serif", "SansSerif", "Monospaced", "Dialog", "DialogInput"];
pub const STYLE_NAMES: [&str; 4] = ["plain", "bold", "italic", "bolditalic"];

/// `CompositeFontDescriptor`.
pub struct CompositeFontDescriptor {
    pub face_name: String,
    pub core_component_count: usize,
    pub component_face_names: Vec<Option<String>>,
    pub component_file_names: Vec<Option<String>>,
    pub exclusion_ranges: Vec<i32>,
    pub exclusion_range_limits: Vec<i32>,
}

#[derive(Default)]
struct Tables {
    script_ids: Vec<String>,
    elc_ids: Vec<String>,
    component_font_name_ids: Vec<String>,
    fontfile_name_ids: Vec<String>,
    filenames: HashMap<i16, i16>,
    sequences: HashMap<i16, Vec<Option<Vec<i16>>>>,
    script_fonts: HashMap<i16, [Option<i16>; 20]>,
    script_allfonts: HashMap<i16, i16>,
    exclusions: HashMap<i16, Vec<i32>>,
    proportionals: Vec<(i16, i16)>,
    alphabetic_suffix: HashMap<i16, String>,
    fallback_script_ids: Vec<i16>,
    version: Option<String>,
    appendedfontpath: Option<String>,
}

fn get_id(map: &mut Vec<String>, key: &str) -> i16 {
    match map.iter().position(|k| k == key) {
        Some(i) => i as i16,
        None => {
            map.push(key.to_string());
            (map.len() - 1) as i16
        }
    }
}

impl Tables {
    fn load(text: &str) -> Tables {
        let mut t = Tables { component_font_name_ids: vec![String::new()], ..Default::default() };
        for (k, v) in parse_properties(text) {
            t.parse_property(&k, &v);
        }
        t
    }

    fn parse_exclusions(exclusions: &str) -> Vec<i32> {
        let num_exclusions = 1 + exclusions.matches(',').count();
        let mut ranges = Vec::with_capacity(num_exclusions * 2);
        for part in exclusions.split(',') {
            let Some((lower, upper)) = part.split_once('-') else { return Vec::new() };
            if !(lower.len() == 4 || lower.len() == 6) || !(upper.len() == 4 || upper.len() == 6) {
                return Vec::new();
            }
            let (Ok(lo), Ok(up)) = (i32::from_str_radix(lower, 16), i32::from_str_radix(upper, 16)) else { return Vec::new() };
            if lo > up {
                return Vec::new();
            }
            ranges.push(lo);
            ranges.push(up);
        }
        ranges
    }

    fn parse_property(&mut self, key: &str, value: &str) {
        if let Some(k) = key.strip_prefix("filename.") {
            let k = if k != "MingLiU_HKSCS" { k.replace('_', " ") } else { k.to_string() };
            let face_id = get_id(&mut self.component_font_name_ids, &k);
            let file_id = get_id(&mut self.fontfile_name_ids, value);
            self.filenames.insert(face_id, file_id);
        } else if let Some(k) = key.strip_prefix("exclusion.") {
            let id = get_id(&mut self.script_ids, k);
            self.exclusions.insert(id, Self::parse_exclusions(value));
        } else if let Some(k) = key.strip_prefix("sequence.") {
            let mut has_default = false;
            let mut has1252 = false;
            let mut sa = Vec::new();
            for s in value.split(',').filter(|s| !s.is_empty()) {
                let s = match s {
                    "alphabetic/default" => {
                        has_default = true;
                        "alphabetic"
                    }
                    "alphabetic/1252" => {
                        has1252 = true;
                        "alphabetic"
                    }
                    s => s,
                };
                sa.push(get_id(&mut self.script_ids, s));
            }
            let (elc_id, k) = match k.find('.') {
                None => {
                    if k == "fallback" {
                        self.fallback_script_ids = sa;
                        return;
                    }
                    if k != "allfonts" {
                        return;
                    }
                    (get_id(&mut self.elc_ids, "NULL.NULL.NULL"), k)
                }
                Some(dot) => (get_id(&mut self.elc_ids, &k[dot + 1..]), &k[..dot]),
            };
            let ids = if k == "allfonts" {
                vec![Some(sa)]
            } else {
                let mut ids = self.sequences.get(&elc_id).cloned().unwrap_or_else(|| vec![None; 5]);
                if ids.len() == 1 {
                    ids = vec![None; 5];
                }
                let Some(fid) = FONT_NAMES.iter().position(|n| *n == k) else { return };
                ids[fid] = Some(sa);
                ids
            };
            self.sequences.insert(elc_id, ids);
            if has_default {
                self.alphabetic_suffix.insert(elc_id, "default".into());
            } else if has1252 {
                self.alphabetic_suffix.insert(elc_id, "1252".into());
            }
        } else if let Some(k) = key.strip_prefix("allfonts.") {
            if k.ends_with(".motif") {
                return;
            }
            let s = get_id(&mut self.script_ids, k);
            let c = get_id(&mut self.component_font_name_ids, value);
            self.script_allfonts.insert(s, c);
        } else if key.starts_with("awtfontpath.") {
            get_id(&mut self.script_ids, &key[12..]);
        } else if key == "version" {
            self.version = Some(value.to_string());
        } else if key == "appendedfontpath" {
            self.appendedfontpath = Some(value.to_string());
        } else if let Some(k) = key.strip_prefix("proportional.") {
            let a = get_id(&mut self.component_font_name_ids, &k.replace('_', " "));
            let b = get_id(&mut self.component_font_name_ids, value);
            self.proportionals.push((a, b));
        } else {
            let Some(dot1) = key.find('.') else { return };
            let Some(dot2) = key[dot1 + 1..].find('.').map(|d| d + dot1 + 1) else { return };
            if key.ends_with(".motif") {
                return;
            }
            let name_id = FONT_NAMES.iter().position(|n| *n == &key[..dot1]);
            let style_id = STYLE_NAMES.iter().position(|n| *n == &key[dot1 + 1..dot2]);
            let script_id = get_id(&mut self.script_ids, &key[dot2 + 1..]);
            let (Some(name_id), Some(style_id)) = (name_id, style_id) else { return };
            let c = get_id(&mut self.component_font_name_ids, value);
            let pnids = self.script_fonts.entry(script_id).or_insert([None; 20]);
            pnids[name_id * NUM_STYLES + style_id] = Some(c);
        }
    }
}

/// `java.util.Properties.load` for the plain ASCII file: `key=value` lines, `#` comments, `\` continuations.
fn parse_properties(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let mut l = line.trim_start().to_string();
        if l.is_empty() || l.starts_with('#') || l.starts_with('!') {
            continue;
        }
        while l.ends_with('\\') {
            l.pop();
            match lines.next() {
                Some(n) => l.push_str(n.trim_start()),
                None => break,
            }
        }
        let sep = l.find(|c: char| c == '=' || c == ':').unwrap_or(l.len());
        let key = l[..sep].trim_end().to_string();
        let value = if sep < l.len() { l[sep + 1..].trim_start().to_string() } else { String::new() };
        out.push((key, value));
    }
    out
}

pub struct FontConfiguration {
    t: Tables,
    init_elc: i16,
    comp_font_name_ids: Vec<Vec<Vec<i16>>>,
    comp_exclusions: Vec<Vec<Vec<i32>>>,
    comp_core_num: [usize; NUM_FONTS],
    core_font_name_ids: JHashSet<i32>,
    fallback_font_name_ids: JHashSet<i32>,
    num_core_fonts: Cell<i32>,
    component_fonts: RefCell<Vec<String>>,
    filenames_map: RefCell<HashMap<String, Option<String>>>,
    pub use_compatibility_fallbacks: bool,
}

const ENCODING: &str = "windows-1252";

impl FontConfiguration {
    /// `new WFontConfiguration(fm)` followed by `init()`.
    pub fn new() -> FontConfiguration {
        let t = Tables::load(FONTCONFIG_PROPERTIES);
        let mut fc = FontConfiguration {
            t,
            init_elc: -1,
            comp_font_name_ids: vec![vec![Vec::new(); NUM_STYLES]; NUM_FONTS],
            comp_exclusions: vec![Vec::new(); NUM_FONTS],
            comp_core_num: [0; NUM_FONTS],
            core_font_name_ids: JHashSet::new(),
            fallback_font_name_ids: JHashSet::new(),
            num_core_fonts: Cell::new(-1),
            component_fonts: RefCell::new(Vec::new()),
            filenames_map: RefCell::new(HashMap::new()),
            use_compatibility_fallbacks: ENCODING == "windows-1252",
        };
        fc.init_font_config();
        fc
    }

    fn init_font_config(&mut self) {
        self.init_elc = self.get_init_elc();
        self.init_all_component_fonts();
    }

    fn get_init_elc(&self) -> i16 {
        let (language, country) = ("en", "US");
        for elc in [format!("{}.{}.{}", ENCODING, language, country), format!("{}.{}", ENCODING, language), ENCODING.to_string()] {
            if let Some(i) = self.t.elc_ids.iter().position(|e| *e == elc) {
                return i as i16;
            }
        }
        self.t.elc_ids.iter().position(|e| e == "NULL.NULL.NULL").unwrap() as i16
    }

    fn get_core_scripts(&self, font_index: usize) -> Vec<i16> {
        let seq = &self.t.sequences[&self.init_elc];
        if seq.len() == 1 {
            seq[0].clone().unwrap_or_default()
        } else {
            seq[font_index].clone().unwrap_or_default()
        }
    }

    fn get_component_font_id(&self, script_id: i16, font_index: usize, style_index: usize) -> i16 {
        if let Some(p) = self.t.script_fonts.get(&script_id) {
            return p[font_index * NUM_STYLES + style_index].unwrap_or(0);
        }
        self.t.script_allfonts.get(&script_id).copied().unwrap_or(0)
    }

    fn get_exclusion_ranges(&self, script_id: i16) -> Vec<i32> {
        self.t.exclusions.get(&script_id).cloned().unwrap_or_default()
    }

    fn init_all_component_fonts(&mut self) {
        let fallback_scripts = self.t.fallback_script_ids.clone();
        for font_index in 0..NUM_FONTS {
            let core_scripts = self.get_core_scripts(font_index);
            self.comp_core_num[font_index] = core_scripts.len();
            self.comp_exclusions[font_index] = core_scripts.iter().map(|&s| self.get_exclusion_ranges(s)).collect();
            for style_index in 0..NUM_STYLES {
                let mut name_ids: Vec<i16> = Vec::new();
                for &s in &core_scripts {
                    let id = self.get_component_font_id(s, font_index, style_index);
                    name_ids.push(id);
                    self.core_font_name_ids.insert(id as i32);
                }
                for &s in &fallback_scripts {
                    let id = self.get_component_font_id(s, font_index, style_index);
                    if name_ids.contains(&id) {
                        continue;
                    }
                    self.fallback_font_name_ids.insert(id as i32);
                    name_ids.push(id);
                }
                self.comp_font_name_ids[font_index][style_index] = name_ids;
            }
        }
    }

    fn get_component_font_name(&self, id: i16) -> Option<String> {
        if id < 0 {
            return None;
        }
        Some(self.t.component_font_name_ids[id as usize].clone())
    }

    fn get_component_file_name(&self, id: i16) -> Option<String> {
        if id < 0 {
            return None;
        }
        Some(self.t.fontfile_name_ids[id as usize].clone())
    }

    fn get_component_file_id(&self, name_id: i16) -> i16 {
        self.t.filenames.get(&name_id).copied().unwrap_or(-1)
    }

    pub fn get_number_core_fonts(&self) -> i32 {
        if self.num_core_fonts.get() == -1 {
            self.num_core_fonts.set(self.core_font_name_ids.len() as i32);
            let core: Vec<i32> = self.core_font_name_ids.iter().copied().collect();
            let fallback: Vec<Option<i32>> = self.fallback_font_name_ids.iter().map(|&f| if self.core_font_name_ids.contains(&f) { None } else { Some(f) }).collect();
            let mut component_fonts = Vec::new();
            let mut filenames_map = self.filenames_map.borrow_mut();
            for fontid in core.into_iter().chain(fallback.into_iter().flatten()) {
                let fontid = fontid as i16;
                let fileid = self.get_component_file_id(fontid);
                let name = self.get_component_font_name(fontid).unwrap_or_default();
                filenames_map.insert(name.clone(), self.get_component_file_name(fileid));
                component_fonts.push(name);
            }
            *self.component_fonts.borrow_mut() = component_fonts;
        }
        self.num_core_fonts.get()
    }

    pub fn get_platform_font_names(&self) -> Vec<String> {
        self.get_number_core_fonts();
        self.component_fonts.borrow().clone()
    }

    pub fn get_file_name_from_platform_name(&self, platform_name: &str) -> Option<String> {
        self.filenames_map.borrow().get(platform_name).cloned().flatten()
    }

    pub fn get_extra_font_path(&self) -> Option<String> {
        self.t.appendedfontpath.clone()
    }

    pub fn get_version(&self) -> Option<String> {
        self.t.version.clone()
    }

    pub fn get_2d_composite_font_info(&self) -> Vec<CompositeFontDescriptor> {
        let mut result = Vec::with_capacity(NUM_FONTS * NUM_STYLES);
        for font_index in 0..NUM_FONTS {
            let font_name = PUBLIC_FONT_NAMES[font_index];
            let exclusions = &self.comp_exclusions[font_index];
            let mut exclusion_ranges = Vec::new();
            let mut exclusion_range_limits = Vec::new();
            for r in exclusions {
                exclusion_ranges.extend_from_slice(r);
                exclusion_range_limits.push(exclusion_ranges.len() as i32);
            }
            for style_index in 0..NUM_STYLES {
                let ids = &self.comp_font_name_ids[font_index][style_index];
                let face_name = format!("{}.{}", font_name, STYLE_NAMES[style_index]);
                let mut component_face_names = Vec::new();
                let mut component_file_names = Vec::new();
                for &font_name_id in ids {
                    let file_name_id = self.get_component_file_id(font_name_id);
                    let cname = self.get_component_font_name(font_name_id);
                    component_face_names.push(cname.clone());
                    let mut f = self.get_component_file_name(file_name_id);
                    if f.is_none() {
                        f = cname.as_deref().and_then(|c| self.get_file_name_from_platform_name(c));
                    }
                    component_file_names.push(f);
                }
                let index = component_file_names.len();
                let mut clipped = exclusion_range_limits.clone();
                if index != clipped.len() {
                    let len = exclusion_range_limits.len();
                    clipped = vec![0; index];
                    clipped[..len].copy_from_slice(&exclusion_range_limits);
                    for c in clipped.iter_mut().skip(len) {
                        *c = exclusion_ranges.len() as i32;
                    }
                }
                result.push(CompositeFontDescriptor { face_name, core_component_count: self.comp_core_num[font_index], component_face_names, component_file_names, exclusion_ranges: exclusion_ranges.clone(), exclusion_range_limits: clipped });
            }
        }
        result
    }

    /// `getCompatibilityFamilyName`.
    pub fn get_compatibility_family_name(&self, font_name: &str) -> Option<String> {
        match font_name.to_lowercase().as_str() {
            "timesroman" => Some("serif".into()),
            "helvetica" => Some("sansserif".into()),
            "courier" => Some("monospaced".into()),
            _ => None,
        }
    }
}

/// The property lines of JDK 17.0.19 `lib/fontconfig.properties.src` (Windows).
static FONTCONFIG_PROPERTIES: &str = r"version=1
allfonts.chinese-ms936=SimSun
allfonts.chinese-ms936-extb=SimSun-ExtB
allfonts.chinese-gb18030=SimSun-18030
allfonts.chinese-gb18030-extb=SimSun-ExtB
allfonts.chinese-hkscs=MingLiU_HKSCS
allfonts.chinese-ms950-extb=MingLiU-ExtB
allfonts.devanagari=Mangal
allfonts.bengali=Vrinda
allfonts.gujarati=Shruti
allfonts.gurmukhi=Raavi
allfonts.kannada=Tunga
allfonts.malayalam=Kartika
allfonts.oriya=Kalinga
allfonts.sinhala=Iskoola Pota
allfonts.tamil=Latha
allfonts.telugu=Gautami
allfonts.khmer=Khmer UI
allfonts.mongolian=Mongolian Baiti
allfonts.myanmar=Myanmar Text
allfonts.dingbats=Wingdings
allfonts.symbol=Symbol
allfonts.symbols=Segoe UI Symbol
allfonts.thai=DokChampa
allfonts.georgian=Sylfaen
serif.plain.alphabetic=Times New Roman
serif.plain.chinese-ms950=MingLiU
serif.plain.chinese-ms950-extb=MingLiU-ExtB
serif.plain.hebrew=David
serif.plain.japanese=MS Mincho
serif.plain.korean=Batang
serif.bold.alphabetic=Times New Roman Bold
serif.bold.chinese-ms950=PMingLiU
serif.bold.chinese-ms950-extb=PMingLiU-ExtB
serif.bold.hebrew=David Bold
serif.bold.japanese=MS Mincho
serif.bold.korean=Batang
serif.italic.alphabetic=Times New Roman Italic
serif.italic.chinese-ms950=PMingLiU
serif.italic.chinese-ms950-extb=PMingLiU-ExtB
serif.italic.hebrew=David
serif.italic.japanese=MS Mincho
serif.italic.korean=Batang
serif.bolditalic.alphabetic=Times New Roman Bold Italic
serif.bolditalic.chinese-ms950=PMingLiU
serif.bolditalic.chinese-ms950-extb=PMingLiU-ExtB
serif.bolditalic.hebrew=David Bold
serif.bolditalic.japanese=MS Mincho
serif.bolditalic.korean=Batang
sansserif.plain.alphabetic=Arial
sansserif.plain.chinese-ms950=MingLiU
sansserif.plain.chinese-ms950-extb=MingLiU-ExtB
sansserif.plain.hebrew=David
sansserif.plain.japanese=MS Gothic
sansserif.plain.korean=Gulim
sansserif.bold.alphabetic=Arial Bold
sansserif.bold.chinese-ms950=PMingLiU
sansserif.bold.chinese-ms950-extb=PMingLiU-ExtB
sansserif.bold.hebrew=David Bold
sansserif.bold.japanese=MS Gothic
sansserif.bold.korean=Gulim
sansserif.italic.alphabetic=Arial Italic
sansserif.italic.chinese-ms950=PMingLiU
sansserif.italic.chinese-ms950-extb=PMingLiU-ExtB
sansserif.italic.hebrew=David
sansserif.italic.japanese=MS Gothic
sansserif.italic.korean=Gulim
sansserif.bolditalic.alphabetic=Arial Bold Italic
sansserif.bolditalic.chinese-ms950=PMingLiU
sansserif.bolditalic.chinese-ms950-extb=PMingLiU-ExtB
sansserif.bolditalic.hebrew=David Bold
sansserif.bolditalic.japanese=MS Gothic
sansserif.bolditalic.korean=Gulim
monospaced.plain.alphabetic=Courier New
monospaced.plain.chinese-ms950=MingLiU
monospaced.plain.chinese-ms950-extb=MingLiU-ExtB
monospaced.plain.hebrew=Courier New
monospaced.plain.japanese=MS Gothic
monospaced.plain.korean=GulimChe
monospaced.bold.alphabetic=Courier New Bold
monospaced.bold.chinese-ms950=PMingLiU
monospaced.bold.chinese-ms950-extb=PMingLiU-ExtB
monospaced.bold.hebrew=Courier New Bold
monospaced.bold.japanese=MS Gothic
monospaced.bold.korean=GulimChe
monospaced.italic.alphabetic=Courier New Italic
monospaced.italic.chinese-ms950=PMingLiU
monospaced.italic.chinese-ms950-extb=PMingLiU-ExtB
monospaced.italic.hebrew=Courier New
monospaced.italic.japanese=MS Gothic
monospaced.italic.korean=GulimChe
monospaced.bolditalic.alphabetic=Courier New Bold Italic
monospaced.bolditalic.chinese-ms950=PMingLiU
monospaced.bolditalic.chinese-ms950-extb=PMingLiU-ExtB
monospaced.bolditalic.hebrew=Courier New Bold
monospaced.bolditalic.japanese=MS Gothic
monospaced.bolditalic.korean=GulimChe
dialog.plain.alphabetic=Arial
dialog.plain.chinese-ms950=MingLiU
dialog.plain.chinese-ms950-extb=MingLiU-ExtB
dialog.plain.hebrew=David
dialog.plain.japanese=MS Gothic
dialog.plain.korean=Gulim
dialog.bold.alphabetic=Arial Bold
dialog.bold.chinese-ms950=PMingLiU
dialog.bold.chinese-ms950-extb=PMingLiU-ExtB
dialog.bold.hebrew=David Bold
dialog.bold.japanese=MS Gothic
dialog.bold.korean=Gulim
dialog.italic.alphabetic=Arial Italic
dialog.italic.chinese-ms950=PMingLiU
dialog.italic.chinese-ms950-extb=PMingLiU-ExtB
dialog.italic.hebrew=David
dialog.italic.japanese=MS Gothic
dialog.italic.korean=Gulim
dialog.bolditalic.alphabetic=Arial Bold Italic
dialog.bolditalic.chinese-ms950=PMingLiU
dialog.bolditalic.chinese-ms950-extb=PMingLiU-ExtB
dialog.bolditalic.hebrew=David Bold
dialog.bolditalic.japanese=MS Gothic
dialog.bolditalic.korean=Gulim
dialoginput.plain.alphabetic=Courier New
dialoginput.plain.chinese-ms950=MingLiU
dialoginput.plain.chinese-ms950-extb=MingLiU-ExtB
dialoginput.plain.hebrew=David
dialoginput.plain.japanese=MS Gothic
dialoginput.plain.korean=Gulim
dialoginput.bold.alphabetic=Courier New Bold
dialoginput.bold.chinese-ms950=PMingLiU
dialoginput.bold.chinese-ms950-extb=PMingLiU-ExtB
dialoginput.bold.hebrew=David Bold
dialoginput.bold.japanese=MS Gothic
dialoginput.bold.korean=Gulim
dialoginput.italic.alphabetic=Courier New Italic
dialoginput.italic.chinese-ms950=PMingLiU
dialoginput.italic.chinese-ms950-extb=PMingLiU-ExtB
dialoginput.italic.hebrew=David
dialoginput.italic.japanese=MS Gothic
dialoginput.italic.korean=Gulim
dialoginput.bolditalic.alphabetic=Courier New Bold Italic
dialoginput.bolditalic.chinese-ms950=PMingLiU
dialoginput.bolditalic.chinese-ms950-extb=PMingLiU-ExtB
dialoginput.bolditalic.hebrew=David Bold
dialoginput.bolditalic.japanese=MS Gothic
dialoginput.bolditalic.korean=Gulim
sequence.allfonts=alphabetic/default,dingbats,symbol
sequence.serif.GBK=alphabetic,chinese-ms936,dingbats,symbol,chinese-ms936-extb
sequence.sansserif.GBK=alphabetic,chinese-ms936,dingbats,symbol,chinese-ms936-extb
sequence.monospaced.GBK=chinese-ms936,alphabetic,dingbats,symbol,chinese-ms936-extb
sequence.dialog.GBK=alphabetic,chinese-ms936,dingbats,symbol,chinese-ms936-extb
sequence.dialoginput.GBK=alphabetic,chinese-ms936,dingbats,symbol,chinese-ms936-extb
sequence.serif.GB18030=alphabetic,chinese-gb18030,dingbats,symbol,chinese-gb18030-extb
sequence.sansserif.GB18030=alphabetic,chinese-gb18030,dingbats,symbol,chinese-gb18030-extb
sequence.monospaced.GB18030=chinese-gb18030,alphabetic,dingbats,symbol,chinese-gb18030-extb
sequence.dialog.GB18030=alphabetic,chinese-gb18030,dingbats,symbol,chinese-gb18030-extb
sequence.dialoginput.GB18030=alphabetic,chinese-gb18030,dingbats,symbol,chinese-gb18030-extb
sequence.serif.x-windows-950=alphabetic,chinese-ms950,dingbats,symbol,chinese-ms950-extb
sequence.sansserif.x-windows-950=alphabetic,chinese-ms950,dingbats,symbol,chinese-ms950-extb
sequence.monospaced.x-windows-950=chinese-ms950,alphabetic,dingbats,symbol,chinese-ms950-extb
sequence.dialog.x-windows-950=alphabetic,chinese-ms950,dingbats,symbol,chinese-ms950-extb
sequence.dialoginput.x-windows-950=alphabetic,chinese-ms950,dingbats,symbol,chinese-ms950-extb
sequence.serif.x-MS950-HKSCS=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.sansserif.x-MS950-HKSCS=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.monospaced.x-MS950-HKSCS=chinese-ms950,alphabetic,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.dialog.x-MS950-HKSCS=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.dialoginput.x-MS950-HKSCS=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.serif.x-MS950-HKSCS-XP=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.sansserif.x-MS950-HKSCS-XP=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.monospaced.x-MS950-HKSCS-XP=chinese-ms950,alphabetic,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.dialog.x-MS950-HKSCS-XP=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.dialoginput.x-MS950-HKSCS-XP=alphabetic,chinese-ms950,chinese-hkscs,dingbats,symbol,chinese-ms950-extb
sequence.allfonts.UTF-8.hi=alphabetic/1252,devanagari,dingbats,symbol
sequence.allfonts.UTF-8.ja=alphabetic,japanese,dingbats,symbol
sequence.allfonts.windows-1255=hebrew,alphabetic/1252,dingbats,symbol
sequence.serif.windows-31j=alphabetic,japanese,dingbats,symbol
sequence.sansserif.windows-31j=alphabetic,japanese,dingbats,symbol
sequence.monospaced.windows-31j=japanese,alphabetic,dingbats,symbol
sequence.dialog.windows-31j=alphabetic,japanese,dingbats,symbol
sequence.dialoginput.windows-31j=alphabetic,japanese,dingbats,symbol
sequence.serif.x-windows-949=alphabetic,korean,dingbats,symbol
sequence.sansserif.x-windows-949=alphabetic,korean,dingbats,symbol
sequence.monospaced.x-windows-949=korean,alphabetic,dingbats,symbol
sequence.dialog.x-windows-949=alphabetic,korean,dingbats,symbol
sequence.dialoginput.x-windows-949=alphabetic,korean,dingbats,symbol
sequence.allfonts.x-windows-874=alphabetic,thai,dingbats,symbol
sequence.fallback=symbols,\
                  chinese-ms950,chinese-hkscs,chinese-ms936,chinese-gb18030,\
                  japanese,korean,chinese-ms950-extb,chinese-ms936-extb,\
                  georgian,devanagari,bengali,gujarati,gurmukhi,kannada,\
                  malayalam,oriya,sinhala,tamil,telugu,thai,khmer,mongolian,\
                  myanmar
exclusion.alphabetic=0700-1cff,1d80-1e9f,1f00-2017,2020-20ab,20ad-20b8,20bb-20bc,20be-f8ff
exclusion.chinese-gb18030=0390-03d6,2200-22ef,2701-27be
exclusion.hebrew=0041-005a,0060-007a,007f-00ff,20ac-20ac
proportional.MS_Gothic=MS PGothic
proportional.MS_Mincho=MS PMincho
proportional.MingLiU=PMingLiU
proportional.MingLiU-ExtB=PMingLiU-ExtB
filename.Arial=ARIAL.TTF
filename.Arial_Bold=ARIALBD.TTF
filename.Arial_Italic=ARIALI.TTF
filename.Arial_Bold_Italic=ARIALBI.TTF
filename.Courier_New=COUR.TTF
filename.Courier_New_Bold=COURBD.TTF
filename.Courier_New_Italic=COURI.TTF
filename.Courier_New_Bold_Italic=COURBI.TTF
filename.Times_New_Roman=TIMES.TTF
filename.Times_New_Roman_Bold=TIMESBD.TTF
filename.Times_New_Roman_Italic=TIMESI.TTF
filename.Times_New_Roman_Bold_Italic=TIMESBI.TTF
filename.SimSun=SIMSUN.TTC
filename.SimSun-18030=SIMSUN18030.TTC
filename.SimSun-ExtB=SIMSUNB.TTF
filename.MingLiU=MINGLIU.TTC
filename.MingLiU-ExtB=MINGLIUB.TTC
filename.PMingLiU=MINGLIU.TTC
filename.PMingLiU-ExtB=MINGLIUB.TTC
filename.MingLiU_HKSCS=hkscsm3u.ttf
filename.David=DAVID.TTF
filename.David_Bold=DAVIDBD.TTF
filename.MS_Mincho=MSMINCHO.TTC
filename.MS_PMincho=MSMINCHO.TTC
filename.MS_Gothic=MSGOTHIC.TTC
filename.MS_PGothic=MSGOTHIC.TTC
filename.Gulim=gulim.TTC
filename.Batang=batang.TTC
filename.GulimChe=gulim.TTC
filename.Gautami=gautami.ttf
filename.Iskoola_Pota=iskpota.ttf
filename.Kalinga=kalinga.ttf
filename.Kartika=kartika.ttf
filename.Latha=latha.ttf
filename.Mangal=MANGAL.TTF
filename.Raavi=raavi.ttf
filename.Shruti=shruti.ttf
filename.Tunga=TUNGA.TTF
filename.Vrinda=vrinda.ttf
filename.DokChampa=dokchamp.ttf
filename.Khmer_UI=KhmerUI.ttf
filename.Mongolian_Baiti=monbaiti.ttf
filename.Myanmar_Text=mmrtext.ttf
filename.Symbol=SYMBOL.TTF
filename.Wingdings=WINGDING.TTF
filename.Sylfaen=sylfaen.ttf
filename.Segoe_UI_Symbol=SEGUISYM.TTF
";
