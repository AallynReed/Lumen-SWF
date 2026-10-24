//! `XFLConverter.convertSWF`: the document, the library symbols and every timeline.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use super::model::{self, Button, Chars, DMatrix, JMatrix, Place, StreamRange};
use super::num::{double_str, JavaRandom};
use super::shapes::{convert_matrix, hex};
use super::xml::{pretty, XmlWriter};
use crate::error::{Error, Result};
use crate::export::Exporter;
use crate::io::Reader;
use crate::morph::{self, MorphShape};
use crate::shape::{self, ShapeTag};
use crate::tags::{code::*, Tag};
use crate::Swf;

pub const KEY_MODE_NORMAL: i32 = 9728;
pub const KEY_MODE_CLASSIC_TWEEN: i32 = 22017;
pub const KEY_MODE_SHAPE_TWEEN: i32 = 17922;
pub const KEY_MODE_MOTION_TWEEN: i32 = 8195;
const PUBLISH_DATA: &str = "PUB_PRST_DATA_EMBED_SWF_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlaVersion {
    Cs5,
    Cs55,
    Cs6,
    Cc,
}

impl FlaVersion {
    pub fn parse(s: &str) -> Option<FlaVersion> {
        Some(match s.to_ascii_lowercase().as_str() {
            "cs5" => FlaVersion::Cs5,
            "cs5.5" | "cs55" => FlaVersion::Cs55,
            "cs6" => FlaVersion::Cs6,
            "cc" => FlaVersion::Cc,
            _ => return None,
        })
    }

    pub fn xfl_version(self) -> &'static str {
        match self {
            FlaVersion::Cs5 => "2.0",
            FlaVersion::Cs55 => "2.1",
            FlaVersion::Cs6 => "2.2",
            FlaVersion::Cc => "2.4",
        }
    }

    pub fn max_swf_version(self) -> u8 {
        match self {
            FlaVersion::Cs5 => 10,
            FlaVersion::Cs55 => 11,
            FlaVersion::Cs6 => 17,
            FlaVersion::Cc => u8::MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// A zipped `.fla` instead of an XFL folder.
    pub compressed: bool,
    pub version: FlaVersion,
    /// Also write AS3 classes and `__Packages` AS2 classes as `.as` files next to the FLA.
    pub export_script: bool,
    /// Seconds since 1970 for `lastModified` and item ids; now when `None`.
    pub timestamp: Option<i64>,
    pub generator: String,
    pub generator_version_name: String,
    pub generator_version: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            compressed: false,
            version: FlaVersion::Cc,
            export_script: true,
            timestamp: None,
            generator: "Lumen-SWF".into(),
            generator_version_name: "Lumen-SWF".into(),
            generator_version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// A timeline entry: a tag of the SWF, a synthetic ShowFrame, or a place whose matrix is reset.
#[derive(Clone, Copy)]
pub enum Item<'a> {
    Tag(&'a Tag),
    Frame,
    NoMatrix(&'a Tag),
}

impl<'a> Item<'a> {
    pub fn code(&self) -> u16 {
        match self {
            Item::Tag(t) | Item::NoMatrix(t) => t.code,
            Item::Frame => SHOW_FRAME,
        }
    }

    pub fn tag(&self) -> Option<&'a Tag> {
        match self {
            Item::Tag(t) | Item::NoMatrix(t) => Some(t),
            Item::Frame => None,
        }
    }

    /// Identity of the tag.
    pub fn key(&self) -> usize {
        match self {
            Item::Tag(t) | Item::NoMatrix(t) => *t as *const Tag as usize,
            Item::Frame => 0,
        }
    }
}

/// A timeline's tags without the End tag.
pub fn items(tags: &[Tag]) -> Vec<Item<'_>> {
    tags.iter().filter(|t| t.code != END).map(Item::Tag).collect()
}

struct MultiLevelClip {
    symbol: i32,
    num_frames: i32,
}

#[derive(Default)]
pub struct Files {
    pub library: BTreeMap<String, Vec<u8>>,
    pub dat: Vec<(String, Vec<u8>)>,
}

pub struct Converter<'a> {
    pub swf: &'a Swf,
    pub ex: Exporter<'a>,
    pub chars: Chars,
    pub version: FlaVersion,
    pub as3: bool,
    rng: JavaRandom,
    names: HashMap<u16, String>,
    last_item: u32,
    pub epoch_hex: String,
    pub timestamp: i64,
    pub files: Files,
    pub characters: Vec<u16>,
    pub classes: HashMap<u16, String>,
    pub variables: HashMap<u16, String>,
    pub first_frame: HashSet<u16>,
    pub non_library: HashSet<u16>,
    pub multi_morphs: Vec<u16>,
    pub small: HashSet<u16>,
    place_cache: RefCell<HashMap<usize, Option<Rc<Place>>>>,
    masked: HashMap<usize, MultiLevelClip>,
    next_clip_id: i32,
    pub ranges: HashMap<i32, (usize, Vec<StreamRange>)>,
    pub range_tags: HashMap<i32, &'a [Tag]>,
    pub scene_offsets: Vec<u32>,
    pub script_ix: std::cell::OnceCell<super::scripts::ScriptIndex>,
    pub font_cache: RefCell<HashMap<u16, Option<Rc<super::text::FontData>>>>,
    rect_cache: RefCell<HashMap<u16, crate::types::Rect>>,
    pub norm: std::cell::OnceCell<super::text::Norm>,
    pub norm_cache: RefCell<HashMap<u16, Option<Rc<super::text::FontData>>>>,
}

impl<'a> Converter<'a> {
    pub fn char_tag(&self, id: u16) -> Option<&'a Tag> {
        self.chars.get(self.swf, id)
    }

    pub fn place(&self, it: &Item<'a>) -> Option<Rc<Place>> {
        let tag = it.tag()?;
        if !model::is_place(tag.code) {
            return None;
        }
        let key = it.key() ^ matches!(it, Item::NoMatrix(_)) as usize;
        if let Some(p) = self.place_cache.borrow().get(&key) {
            return p.clone();
        }
        let mut p = Place::read(tag);
        if let (Item::NoMatrix(_), Some(pl)) = (it, p.as_mut()) {
            if pl.matrix.is_some() {
                pl.matrix = Some(JMatrix::default());
            }
        }
        let p = p.map(Rc::new);
        self.place_cache.borrow_mut().insert(key, p.clone());
        p
    }

    fn random_color(&mut self) -> String {
        loop {
            let r = self.rng.next_int(256);
            let g = self.rng.next_int(256);
            let b = self.rng.next_int(256);
            if (r + g + b) / 3 >= 128 {
                return format!("#{:02x}{:02x}{:02x}", r, g, b);
            }
        }
    }

    pub fn item_id(&mut self) -> String {
        self.last_item += 1;
        format!("{}-{:08x}", self.epoch_hex, self.last_item)
    }

    /// `getSymbolName`: the library name, fixed by the first kind it is asked with.
    pub fn symbol_name(&mut self, id: u16, kind: &str) -> String {
        self.names.entry(id).or_insert_with(|| format!("{kind} {id}")).clone()
    }

    pub fn image_ext(&self, id: u16) -> Option<&'static str> {
        let tag = self.char_tag(id)?;
        if !crate::export::image::is_image(tag.code) {
            return None;
        }
        let jt = self.swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
        let bmp = crate::export::image::Bitmap::read(tag, jt).ok()?;
        Some(match bmp.native_format() {
            crate::export::image::Format::Jpeg => ".jpg",
            crate::export::image::Format::Gif => ".gif",
            _ => ".png",
        })
    }

    pub fn bitmap_path(&mut self, id: u16) -> Option<String> {
        let ext = self.image_ext(id)?;
        Some(format!("{}{ext}", self.symbol_name(id, "Bitmap")))
    }

    fn timestamp_str(&self) -> String {
        self.timestamp.to_string()
    }


    fn is_morph(&self, id: u16) -> bool {
        self.char_tag(id).is_some_and(|t| morph::is_morph(t.code))
    }
}

// ---- usage analysis ----

#[derive(Default, Clone, Copy)]
struct DepthFlags {
    name: bool,
    cx: bool,
    cache: bool,
    matrix: bool,
}

fn walk_shape_usages(c: &Converter, tags: &[Tag], usages: &mut HashMap<u16, i32>) {
    let mut depth_map: HashMap<i32, i32> = HashMap::new();
    let mut flags: HashMap<i32, DepthFlags> = HashMap::new();
    for t in tags {
        if let Some(s) = &t.sprite {
            walk_shape_usages(c, &s.tags, usages);
        }
        if let Some(d) = model::remove_depth(t) {
            depth_map.remove(&d);
            flags.remove(&d);
        }
        let Some(po) = c.place(&Item::Tag(t)) else { continue };
        let d = po.depth;
        if po.is_move != depth_map.contains_key(&d) {
            continue;
        }
        if !po.is_move {
            flags.insert(d, DepthFlags::default());
        }
        let f = flags.entry(d).or_default();
        if po.name.is_some() {
            f.name = true;
        } else if po.cxform.is_some() {
            f.cx = true;
        } else if po.cache_as_bitmap {
            f.cache = true;
        } else if po.matrix.is_some_and(|m| !m.is_empty()) {
            f.matrix = true;
        }
        let f = *f;
        let mut ch = po.char_id;
        if ch == -1 {
            if let Some(&x) = depth_map.get(&d) {
                ch = x;
            }
        } else {
            depth_map.insert(d, ch);
        }
        if ch == -1 {
            continue;
        }
        let u = usages.entry(ch as u16).or_insert(0);
        *u += 1;
        if f.name || f.cx || f.cache || f.matrix {
            *u += 1;
        }
    }
}

fn walk_morph_usages(c: &Converter, tags: &[Tag], usages: &mut BTreeMap<u16, i32>) {
    let mut depth_map: HashMap<i32, i32> = HashMap::new();
    for t in tags {
        if let Some(s) = &t.sprite {
            walk_morph_usages(c, &s.tags, usages);
        }
        if let Some(d) = model::remove_depth(t) {
            depth_map.remove(&d);
        }
        let Some(po) = c.place(&Item::Tag(t)) else { continue };
        let d = po.depth;
        if po.is_move != depth_map.contains_key(&d) {
            continue;
        }
        let mut ch = po.char_id;
        if ch == -1 {
            if let Some(&x) = depth_map.get(&d) {
                ch = x;
            }
        } else {
            depth_map.insert(d, ch);
        }
        if ch == -1 {
            continue;
        }
        if c.is_morph(ch as u16) {
            let u = usages.entry(ch as u16).or_insert(0);
            if po.ratio <= 0 {
                *u += 1;
            }
        }
    }
}

fn has_new_styles(records: &[shape::Record]) -> bool {
    records.iter().any(|r| matches!(r, shape::Record::StyleChange { new_styles: Some(_), .. }))
}

// ---- top level ----

pub struct Output {
    /// Files of the XFL document, relative to its folder.
    pub fla: Vec<(String, Vec<u8>)>,
    /// `.as` files for the scripts folder next to the document.
    pub scripts: Vec<(String, Vec<u8>)>,
    pub has_import: bool,
}

/// Converts `swf` to XFL. `base_name` names the publish files (`<base_name>.swf`).
pub fn convert(swf: &Swf, base_name: &str, opts: &Options) -> Result<Output> {
    let as3 = swf.is_as3();
    let network = swf.timeline.tags.iter().find(|t| t.code == FILE_ATTRIBUTES).and_then(|t| t.body.first()).is_some_and(|f| f & 0x01 != 0);
    if !as3 && opts.version == FlaVersion::Cc {
        return Err(Error::Other("FLA version CC does not support AS1/2".into()));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let ts = opts.timestamp.unwrap_or(now);
    let mut c = Converter {
        swf,
        ex: Exporter::new(swf),
        chars: Chars::of(swf),
        version: opts.version,
        as3,
        rng: JavaRandom::new(123),
        names: HashMap::new(),
        last_item: 0,
        epoch_hex: format!("{:08x}", now),
        timestamp: ts,
        files: Files::default(),
        characters: Vec::new(),
        classes: HashMap::new(),
        variables: HashMap::new(),
        first_frame: HashSet::new(),
        non_library: HashSet::new(),
        multi_morphs: Vec::new(),
        small: HashSet::new(),
        place_cache: RefCell::new(HashMap::new()),
        masked: HashMap::new(),
        next_clip_id: -1,
        ranges: HashMap::new(),
        range_tags: HashMap::new(),
        scene_offsets: Vec::new(),
        script_ix: Default::default(),
        font_cache: RefCell::new(HashMap::new()),
        rect_cache: RefCell::new(HashMap::new()),
        norm: Default::default(),
        norm_cache: RefCell::new(HashMap::new()),
    };
    c.prepare();

    let bg = swf.timeline.tags.iter().find(|t| t.code == SET_BACKGROUND_COLOR).and_then(|t| crate::types::Rgba::read_rgb(&mut Reader::new(&t.body)).ok()).map(|c| hex(&c)).unwrap_or_else(|| "#ffffff".into());
    let r = &swf.frame_rect;
    let width = (r.x_max - r.x_min) as f64 / 20.0;
    let height = (r.y_max - r.y_min) as f64 / 20.0;
    let mut doc = XmlWriter::new();
    doc.start_with(
        "DOMDocument",
        &[
            ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
            ("xmlns", "http://ns.adobe.com/xfl/2008/"),
            ("currentTimeline", "1"),
            ("xflVersion", opts.version.xfl_version()),
            ("creatorInfo", &opts.generator),
            ("platform", "Windows"),
            ("versionInfo", &format!("Saved by {}", opts.generator_version_name)),
            ("majorVersion", &opts.generator_version),
            ("buildNumber", ""),
            ("nextSceneIdentifier", "2"),
            ("playOptionsPlayLoop", "false"),
            ("playOptionsPlayPages", "false"),
            ("playOptionsPlayFrameActions", "false"),
            ("autoSaveHasPrompted", "true"),
            ("backgroundColor", &bg),
            ("frameRate", &double_str(swf.frame_rate as f64)),
        ],
    );
    let doc_pack = c.document_pack();
    if let Some(pack) = &doc_pack {
        let acc = c.root_accessibility(pack);
        if !acc.is_empty() {
            doc.attr("hasAccessibleData", "true");
            for (k, v) in &acc {
                doc.attr(k, v);
            }
        }
    }
    if width.total_cmp(&550.0).is_ne() {
        doc.attr("width", &double_str(width));
    }
    if height.total_cmp(&400.0).is_ne() {
        doc.attr("height", &double_str(height));
    }
    let has_import = swf.timeline.tags.iter().any(|t| t.code == IMPORT_ASSETS || t.code == IMPORT_ASSETS2);
    if has_import {
        doc.start("folders");
        let id = c.item_id();
        doc.empty_with("DOMFolderItem", &[("name", "imported"), ("itemID", &id), ("isExpanded", "false")]);
        doc.end();
    }
    c.convert_fonts(&mut doc);
    c.convert_media(&mut doc);
    c.convert_symbols(&mut doc);
    let root_items = items(&swf.timeline.tags);
    c.convert_timelines(None, -1, None, &root_items, None, &mut doc, doc_pack.as_ref());
    if c.has_amf_metadata(&swf.timeline.tags) {
        doc.start("persistentData");
        doc.start("PD");
        doc.attr("n", PUBLISH_DATA);
        doc.attr("t", "i");
        doc.attr_i("v", 1);
        doc.end();
        doc.end();
    }
    doc.end();
    let dom = pretty(doc.as_str());

    let mut scripts = Vec::new();
    if opts.export_script {
        scripts = c.export_scripts();
    }
    let publish = c.publish_settings(base_name, width, height, network, &doc_pack);
    let mut fla = vec![("DOMDocument.xml".to_string(), dom.into_bytes()), ("PublishSettings.xml".to_string(), publish.into_bytes())];
    for (k, v) in std::mem::take(&mut c.files.library) {
        fla.push((format!("LIBRARY/{k}"), v));
    }
    for (k, v) in std::mem::take(&mut c.files.dat) {
        if !v.is_empty() {
            fla.push((format!("bin/{k}"), v));
        }
    }
    Ok(Output { fla, scripts, has_import })
}

/// Writes an XFL folder (`dir/<base>/<base>.xfl` and its files) or a zipped `dir/<base>.fla`,
/// and the script files into `dir`.
pub fn write(out: &Output, dir: &std::path::Path, base_name: &str, compressed: bool) -> Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    if compressed {
        let file = std::fs::File::create(dir.join(format!("{base_name}.fla")))?;
        let mut z = zip::ZipWriter::new(file);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let err = |e: zip::result::ZipError| Error::Other(format!("ZIP: {e}"));
        for (i, (name, data)) in out.fla.iter().enumerate() {
            if i == 2 {
                z.add_directory("LIBRARY/", o).map_err(err)?;
            }
            z.start_file(name.as_str(), o).map_err(err)?;
            z.write_all(data)?;
        }
        if out.fla.len() <= 2 {
            z.add_directory("LIBRARY/", o).map_err(err)?;
        }
        if out.has_import {
            z.add_directory("LIBRARY/imported/", o).map_err(err)?;
        }
        z.finish().map_err(err)?;
    } else {
        let root = dir.join(base_name);
        std::fs::create_dir_all(root.join("LIBRARY"))?;
        std::fs::create_dir_all(root.join("bin"))?;
        if out.has_import {
            std::fs::create_dir_all(root.join("LIBRARY").join("imported"))?;
        }
        for (name, data) in &out.fla {
            let p = root.join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, data)?;
        }
        std::fs::write(root.join(format!("{base_name}.xfl")), b"PROXY-CS5")?;
    }
    for (name, data) in &out.scripts {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(p, data)?;
    }
    Ok(())
}

impl<'a> Converter<'a> {
    fn prepare(&mut self) {
        let swf = self.swf;
        self.characters = self.chars.order.clone();
        let names = crate::names::Names::new(swf);
        for &id in &self.characters {
            if let Some(cl) = names.classes.get(&id) {
                let mut uniq: Vec<&String> = Vec::new();
                for c in cl {
                    if !uniq.contains(&c) {
                        uniq.push(c);
                    }
                }
                if uniq.len() == 1 {
                    self.classes.insert(id, uniq[0].clone());
                }
            }
        }
        for t in &swf.timeline.tags {
            if t.code == EXPORT_ASSETS {
                let mut r = Reader::new(&t.body);
                let n = r.u16().unwrap_or(0);
                for _ in 0..n {
                    let (Ok(id), Ok(name)) = (r.u16(), r.string()) else { break };
                    self.variables.entry(id).or_insert(name);
                }
            }
        }
        for t in &swf.timeline.tags {
            if t.code == SHOW_FRAME {
                break;
            }
            if t.code == EXPORT_ASSETS || t.code == SYMBOL_CLASS {
                let mut r = Reader::new(&t.body);
                let n = r.u16().unwrap_or(0);
                for _ in 0..n {
                    let (Ok(id), Ok(_)) = (r.u16(), r.string()) else { break };
                    if t.code == SYMBOL_CLASS && id == 0 {
                        continue;
                    }
                    if self.char_tag(id).is_some() {
                        self.first_frame.insert(id);
                    }
                }
            }
        }
        let mut usages = HashMap::new();
        walk_shape_usages(self, &swf.timeline.tags, &mut usages);
        for (id, n) in usages {
            if n < 2 {
                if let Some(t) = self.char_tag(id) {
                    if shape::is_shape(t.code) {
                        if let Ok(s) = ShapeTag::read(t) {
                            if !has_new_styles(&s.shape.records) {
                                self.non_library.insert(id);
                            }
                        }
                    }
                }
            }
        }
        let mut mu = BTreeMap::new();
        walk_morph_usages(self, &swf.timeline.tags, &mut mu);
        self.multi_morphs = mu.into_iter().filter(|(_, n)| *n > 1).map(|(id, _)| id).collect();
        for &id in &self.characters {
            if let Some(t) = self.char_tag(id) {
                if shape::is_shape(t.code) {
                    if let Ok(s) = ShapeTag::read(t) {
                        let b = s.bounds;
                        if b.x_max - b.x_min < 1000 && b.y_max - b.y_min < 1000 {
                            self.small.insert(id);
                        }
                    }
                }
            }
        }
        for t in &swf.timeline.tags {
            if t.code == DEFINE_SCENE_AND_FRAME_LABEL_DATA {
                self.scene_offsets = scene_data(t).0.into_iter().map(|s| s.0).collect();
            }
        }
        let offsets = self.scene_offsets.clone();
        fn walk_ranges<'b>(c: &mut Converter<'b>, container: i32, tags: &'b [Tag], offsets: &[u32]) {
            model::stream_ranges(container, tags, offsets, &mut c.ranges);
            if c.ranges.contains_key(&container) {
                c.range_tags.insert(container, tags);
            }
            for t in tags {
                if let Some(s) = &t.sprite {
                    let id = model::character_id(t).map_or(-1, |i| i as i32);
                    walk_ranges(c, id, &s.tags, &[]);
                }
            }
        }
        walk_ranges(self, -1, &swf.timeline.tags, &offsets);
    }

    fn has_amf_metadata(&self, tags: &[Tag]) -> bool {
        tags.iter().any(|t| self.place(&Item::Tag(t)).is_some_and(|p| p.amf.is_some()) || t.sprite.as_ref().is_some_and(|s| self.has_amf_metadata(&s.tags)))
    }

    // ---- library ----

    pub fn write_linkage(&self, w: &mut XmlWriter, id: u16) {
        let mut export = false;
        if let Some(cl) = self.classes.get(&id) {
            export = true;
            w.attr("linkageClassName", cl);
        }
        if let Some(v) = self.variables.get(&id) {
            export = true;
            w.attr("linkageIdentifier", v);
        }
        if export {
            if !self.first_frame.contains(&id) {
                w.attr("linkageExportInFirstFrame", "false");
            }
            w.attr_bool("linkageExportForAS", true);
        }
    }

    fn include(&mut self, w: &mut XmlWriter, file: &str, item_id: &str, icon: Option<&str>) {
        w.start_with("Include", &[("href", file)]);
        w.attr("itemID", item_id);
        if let Some(i) = icon {
            w.attr("itemIcon", i);
        }
        w.attr_bool("loadImmediate", false);
        if self.version >= FlaVersion::Cs55 {
            w.attr_i("lastModified", self.timestamp);
        }
        w.end();
    }

    fn symbol_item_start(&mut self, w: &mut XmlWriter, name: &str, item_id: &str) {
        w.start_with(
            "DOMSymbolItem",
            &[
                ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
                ("xmlns", "http://ns.adobe.com/xfl/2008/"),
                ("name", name),
                ("itemID", item_id),
                ("lastModified", &self.timestamp_str()),
            ],
        );
    }

    fn convert_symbols(&mut self, w: &mut XmlWriter) {
        w.start("symbols");
        let chars = self.characters.clone();
        for id in chars {
            let Some(tag) = self.char_tag(id) else { continue };
            let code = tag.code;
            let is_shape = shape::is_shape(code);
            if is_shape && self.non_library.contains(&id) {
                continue;
            }
            let is_button = code == DEFINE_BUTTON || code == DEFINE_BUTTON2;
            if !(is_shape || code == DEFINE_SPRITE || is_button) {
                continue;
            }
            if code == DEFINE_SPRITE && tag.sprite.as_ref().is_none_or(|s| s.tags.iter().all(|t| t.code == END)) {
                continue;
            }
            let mut sym = XmlWriter::new();
            let item_id = self.item_id();
            let name = self.symbol_name(id, "Symbol");
            self.symbol_item_start(&mut sym, &name, &item_id);
            self.write_linkage(&mut sym, id);
            if is_shape {
                sym.attr("symbolType", "graphic");
            } else if is_button {
                sym.attr("symbolType", "button");
                if Button::read(tag).is_some_and(|b| b.track_as_menu) {
                    sym.attr_bool("trackAsMenu", true);
                }
            }
            if let Some(grid) = self.scaling_grid(id) {
                sym.attr("scaleGridLeft", &double_str(grid.x_min as f64 / 20.0));
                sym.attr("scaleGridRight", &double_str(grid.x_max as f64 / 20.0));
                sym.attr("scaleGridTop", &double_str(grid.y_min as f64 / 20.0));
                sym.attr("scaleGridBottom", &double_str(grid.y_max as f64 / 20.0));
            }
            let mut icon = None;
            let short = name.rsplit('/').next().unwrap_or(&name).to_string();
            if is_button {
                icon = Some("0");
                self.convert_button(tag, id, &short, &mut sym);
            } else if code == DEFINE_SPRITE {
                let sprite_items = items(&tag.sprite.as_ref().unwrap().tags);
                let pack = self.character_pack(id);
                self.extract_multilevel_clips(&sprite_items, id as i32, w);
                self.convert_timelines(Some(id), id as i32, self.variables.get(&id).cloned().as_deref(), &sprite_items, Some(&name), &mut sym, pack.as_ref());
            } else {
                icon = Some("1");
                sym.start("timeline");
                sym.start_with("DOMTimeline", &[("name", &short), ("currentFrame", "0")]);
                sym.start("layers");
                if let Ok(s) = ShapeTag::read(tag) {
                    let small = self.small.contains(&id);
                    self.convert_shape(None, s.shape_num, &s.shape, false, true, &mut sym, small);
                }
                sym.end();
                sym.end();
                sym.end();
            }
            sym.end();
            let file = format!("{name}.xml");
            self.files.library.insert(file.clone(), pretty(sym.as_str()).into_bytes());
            self.include(w, &file, &item_id, icon);
        }
        let root_items = items(&self.swf.timeline.tags);
        self.extract_multilevel_clips(&root_items, -1, w);
        self.extract_multi_usage_morphs(w);
        w.end();
    }

    fn scaling_grid(&self, id: u16) -> Option<crate::types::Rect> {
        let mut found = None;
        self.swf.walk(|_, t| {
            if t.code == DEFINE_SCALING_GRID && found.is_none() {
                let mut r = Reader::new(&t.body);
                if r.u16().ok() == Some(id) {
                    found = crate::types::Rect::read(&mut r).ok();
                }
            }
        });
        found
    }

    fn convert_button(&mut self, tag: &'a Tag, id: u16, short: &str, w: &mut XmlWriter) {
        let Some(button) = Button::read(tag) else { return };
        w.start("timeline");
        w.start_with("DOMTimeline", &[("name", short), ("currentFrame", "0")]);
        w.start("layers");
        let max_depth = button.records.iter().map(|r| r.depth).max().unwrap_or(0).max(0);
        let sounds = self.button_sounds(id);
        let mut sound_offset = 0;
        if let Some(sounds) = &sounds {
            sound_offset = 1;
            w.start_with("DOMLayer", &[("name", "Layer 1")]);
            w.start("frames");
            for (frame, (sound_char, info)) in sounds.iter().enumerate() {
                w.start_with("DOMFrame", &[("index", &frame.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                if *sound_char > 0 {
                    if let Some(st) = self.char_tag(*sound_char) {
                        if st.code == DEFINE_SOUND {
                            self.convert_sound_usage(w, st, info);
                        }
                    }
                }
                w.start("elements");
                w.end();
                w.end();
            }
            w.end();
            w.end();
        }
        let cx_tag = self.button_cxform(id);
        for i in (1..=max_depth).rev() {
            w.start_with("DOMLayer", &[("name", &format!("Layer {}", max_depth - i + 1 + sound_offset))]);
            if i == 1 {
                w.attr_bool("current", true);
                w.attr_bool("isSelected", true);
            }
            let color = self.random_color();
            w.attr("color", &color);
            w.start("frames");
            let mut last_frame = 0;
            let mut frame = 1;
            while frame <= 4 {
                for rec in &button.records {
                    if rec.depth != i {
                        continue;
                    }
                    let mut duration = 0;
                    while frame + duration <= 4 {
                        let ok = match frame + duration {
                            1 => rec.up,
                            2 => rec.over,
                            3 => rec.down,
                            _ => rec.hit,
                        };
                        if !ok {
                            break;
                        }
                        duration += 1;
                    }
                    if duration == 0 {
                        continue;
                    }
                    let cx = if button.v2 { rec.cxform } else { None };
                    let blend = rec.blend.map_or(0, |b| b as i32);
                    let filters = if rec.has_filters { Some(rec.filters.clone()) } else { Some(Vec::new()) };
                    let Some(ch) = self.char_tag(rec.char_id) else { continue };
                    let mut rw = XmlWriter::new();
                    let m = rec.matrix;
                    if shape::is_shape(ch.code) && self.non_library.contains(&rec.char_id) {
                        if let Ok(s) = ShapeTag::read(ch) {
                            self.convert_shape(Some(&m), s.shape_num, &s.shape, false, false, &mut rw, false);
                        }
                    } else if morph::is_morph(ch.code) {
                        if let Ok(ms) = MorphShape::read(ch) {
                            let num = if ms.version == 1 { 3 } else { 4 };
                            self.convert_shape(Some(&m), num, &ms.start_shape(), true, false, &mut rw, false);
                        }
                    } else if matches!(ch.code, DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT) {
                        self.convert_text(frame as i32, &super::scripts::Accessibility::default(), None, ch, &m, filters.as_deref(), &mut rw);
                    } else if ch.code == DEFINE_VIDEO_STREAM {
                        self.convert_video_instance(None, &m, ch, &mut rw);
                    } else if crate::export::image::is_image(ch.code) {
                        self.convert_image_instance(None, &m, rec.char_id, &mut rw);
                    } else {
                        let small = self.small.contains(&rec.char_id);
                        let inst = Instance { matrix: Some(m), cxform: cx.map(Cxf::Alpha), blend, filters: filters.clone(), visible: true, ..Default::default() };
                        self.convert_symbol_instance(-1, &super::scripts::Accessibility::default(), &inst, rec.char_id, &mut rw, small, cx_tag);
                    }
                    let empty = frame - last_frame - 1;
                    last_frame = frame + duration - 1;
                    if empty > 0 {
                        w.start_with("DOMFrame", &[("index", &(frame - empty).to_string()), ("duration", &empty.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                        w.value("elements", "");
                        w.end();
                    }
                    if duration > 1 {
                        w.start_with("DOMFrame", &[("index", &frame.to_string()), ("duration", &duration.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                    } else {
                        w.start_with("DOMFrame", &[("index", &frame.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                    }
                    w.start("elements");
                    w.raw(rw.as_str());
                    w.end();
                    w.end();
                    frame += duration - 1;
                }
                frame += 1;
            }
            w.end();
            w.end();
        }
        w.end();
        w.end();
        w.end();
    }

    fn button_sounds(&self, id: u16) -> Option<Vec<(u16, model::SoundInfo)>> {
        let mut found = None;
        self.swf.walk(|_, t| {
            if t.code == DEFINE_BUTTON_SOUND && found.is_none() {
                let mut r = Reader::new(&t.body);
                if r.u16().ok() != Some(id) {
                    return;
                }
                let mut v = Vec::new();
                for _ in 0..4 {
                    let ch = r.u16().unwrap_or(0);
                    let info = if ch != 0 { model::SoundInfo::read(&mut r).unwrap_or_default() } else { model::SoundInfo::default() };
                    v.push((ch, info));
                }
                found = Some(v);
            }
        });
        found
    }

    fn button_cxform(&self, id: u16) -> Option<model::Cx> {
        let tag = self.char_tag(id)?;
        if tag.code != DEFINE_BUTTON {
            return None;
        }
        let mut found = None;
        self.swf.walk(|_, t| {
            if t.code == DEFINE_BUTTON_CXFORM && found.is_none() {
                let mut r = Reader::new(&t.body);
                if r.u16().ok() == Some(id) {
                    found = model::Cx::read(&mut r, false).ok();
                }
            }
        });
        found
    }

    // ---- multilevel masks and morph symbols ----

    fn add_extracted_clip(&mut self, items: &[Item<'a>], sprite_id: i32, w: &mut XmlWriter) {
        self.extract_multilevel_clips(items, sprite_id, w);
        self.next_clip_id = if self.next_clip_id < 0 { self.next_character_id() } else { self.next_clip_id + 1 };
        let object_id = self.next_clip_id;
        let item_id = self.item_id();
        let mut sym = XmlWriter::new();
        let name = format!("Symbol {object_id}");
        self.symbol_item_start(&mut sym, &name, &item_id);
        sym.attr("symbolType", "graphic");
        self.convert_timelines(None, object_id, Some(""), items, Some(&name), &mut sym, None);
        sym.end();
        let file = format!("{name}.xml");
        self.files.library.insert(file.clone(), pretty(sym.as_str()).into_bytes());
        self.include(w, &file, &item_id, Some("1"));
    }

    fn next_character_id(&self) -> i32 {
        self.chars.by_id.keys().map(|&k| k as i32).max().unwrap_or(0) + 1
    }

    fn extract_multilevel_clips(&mut self, items: &[Item<'a>], sprite_id: i32, w: &mut XmlWriter) {
        let mut f = 0i32;
        let mut clip_places: Vec<Item<'a>> = Vec::new();
        let mut depth_to_clip: HashMap<i32, usize> = HashMap::new();
        let mut finish: HashMap<usize, i32> = HashMap::new();
        let mut start: HashMap<usize, i32> = HashMap::new();
        let mut occupied: HashSet<i32> = HashSet::new();
        let mut current: Vec<Item<'a>> = Vec::new();
        let max_depth = self.max_depth(items);
        let mut last: Option<Item<'a>> = None;
        let sort = |c: &Converter<'a>, v: &mut Vec<Item<'a>>| v.sort_by(|a, b| c.place(b).unwrap().clip_depth.cmp(&c.place(a).unwrap().clip_depth));
        for it in items {
            if it.code() == SHOW_FRAME {
                sort(self, &mut current);
                clip_places.append(&mut current);
                f += 1;
            }
            if let Some(po) = self.place(it) {
                if po.is_move == occupied.contains(&po.depth) {
                    if !po.is_move || po.char_id != -1 || po.class_name.is_some() {
                        occupied.insert(po.depth);
                    }
                    if po.clip_depth > -1 {
                        start.insert(it.key(), f);
                        current.push(*it);
                        if let Some(&k) = depth_to_clip.get(&po.depth) {
                            finish.insert(k, f - 1);
                        }
                        depth_to_clip.insert(po.depth, it.key());
                    } else if !po.is_move {
                        if let Some(k) = depth_to_clip.remove(&po.depth) {
                            finish.insert(k, f - 1);
                        }
                    }
                }
            }
            if let Some(d) = it.tag().and_then(model::remove_depth) {
                occupied.remove(&d);
                if let Some(k) = depth_to_clip.remove(&d) {
                    finish.insert(k, f - 1);
                }
            }
            last = Some(*it);
        }
        sort(self, &mut current);
        clip_places.append(&mut current);
        if clip_places.is_empty() {
            return;
        }
        if last.is_some_and(|l| l.code() != SHOW_FRAME) {
            f += 1;
        }
        let frame_count = f;
        for &k in depth_to_clip.values() {
            finish.insert(k, frame_count - 1);
        }
        let mut frame_to_depth: BTreeMap<i32, BTreeMap<i32, Vec<Item<'a>>>> = BTreeMap::new();
        for fr in 0..frame_count {
            for d in 0..max_depth {
                for po_it in &clip_places {
                    let po = self.place(po_it).unwrap();
                    let (s, e) = (start[&po_it.key()], finish.get(&po_it.key()).copied().unwrap_or(frame_count - 1));
                    if fr >= s && fr <= e && d >= po.depth && d <= po.clip_depth {
                        frame_to_depth.entry(fr).or_default().entry(d).or_default().push(*po_it);
                    }
                }
            }
        }
        let mut delegated: HashSet<usize> = HashSet::new();
        for (&fr, depths) in &frame_to_depth {
            for places in depths.values() {
                if places.len() <= 1 {
                    continue;
                }
                let first = places[0];
                let second = places[1];
                if delegated.contains(&second.key()) {
                    continue;
                }
                delegated.insert(second.key());
                let sp = self.place(&second).unwrap();
                let mut timeline: Vec<Item<'a>> = Vec::new();
                let mut f = 0;
                let mut removed = false;
                let mut num_frames = 0;
                let mut last: Option<Item<'a>> = None;
                let mut occ: HashSet<i32> = HashSet::new();
                for it in items {
                    if f < fr {
                        if let Some(p) = self.place(it) {
                            if p.is_move != occ.contains(&p.depth) {
                                continue;
                            }
                            if p.char_id != -1 && p.class_name.is_some() {
                                occ.insert(p.depth);
                            }
                        }
                        if let Some(d) = it.tag().and_then(model::remove_depth) {
                            occ.remove(&d);
                        }
                    }
                    if f >= fr {
                        if let Some(p) = self.place(it) {
                            if p.is_move == occ.contains(&p.depth) {
                                if p.char_id != -1 && p.class_name.is_some() {
                                    occ.insert(p.depth);
                                }
                                if p.depth == sp.depth {
                                    if p.is_move {
                                        removed = false;
                                    } else if p.clip_depth == sp.clip_depth {
                                        removed = false;
                                        delegated.insert(it.key());
                                    } else {
                                        removed = true;
                                    }
                                }
                                if !removed && it.key() != first.key() && p.depth >= sp.depth && p.depth <= sp.clip_depth {
                                    timeline.push(*it);
                                }
                            }
                        }
                        if let Some(d) = it.tag().and_then(model::remove_depth) {
                            if d == sp.depth {
                                removed = true;
                            }
                            occ.remove(&d);
                        }
                    }
                    last = Some(*it);
                    if it.code() == SHOW_FRAME {
                        if f >= fr {
                            timeline.push(*it);
                            num_frames += 1;
                        }
                        if removed {
                            break;
                        }
                        f += 1;
                    }
                }
                if last.is_none_or(|l| l.code() != SHOW_FRAME) {
                    num_frames += 1;
                    timeline.push(Item::Frame);
                }
                self.add_extracted_clip(&timeline, sprite_id, w);
                self.masked.insert(second.key(), MultiLevelClip { symbol: self.next_clip_id, num_frames });
            }
        }
    }

    fn morph_timeline(&self, morph_id: u16, tags: &'a [Tag], out: &mut Vec<Item<'a>>) -> bool {
        for t in tags {
            if let Some(s) = &t.sprite {
                if self.morph_timeline(morph_id, &s.tags, out) {
                    return true;
                }
            }
        }
        let mut morph_depth = -2;
        let mut on_track = false;
        let mut was = false;
        let mut last_ratio = -1;
        for t in tags {
            let it = Item::Tag(t);
            if let Some(p) = self.place(&it) {
                if morph_depth == p.depth && p.char_id != -1 && p.char_id != morph_id as i32 {
                    out.push(it);
                    on_track = false;
                } else if (morph_depth == -2 && p.char_id == morph_id as i32) || morph_depth == p.depth {
                    morph_depth = p.depth;
                    if on_track && p.ratio < last_ratio {
                        on_track = false;
                    } else {
                        on_track = true;
                        was = true;
                        out.push(it);
                        last_ratio = p.ratio;
                    }
                }
            }
            if model::remove_depth(t) == Some(morph_depth) {
                if on_track {
                    out.push(it);
                }
                on_track = false;
            }
            if t.code == SHOW_FRAME {
                if on_track {
                    out.push(it);
                }
                if was && !on_track {
                    out.push(it);
                    break;
                }
            }
        }
        !out.is_empty()
    }

    fn extract_multi_usage_morphs(&mut self, w: &mut XmlWriter) {
        for id in self.multi_morphs.clone() {
            let item_id = self.item_id();
            let name = self.symbol_name(id, "Symbol");
            let mut sym = XmlWriter::new();
            self.symbol_item_start(&mut sym, &name, &item_id);
            sym.attr("symbolType", "graphic");
            let mut tl = Vec::new();
            self.morph_timeline(id, &self.swf.timeline.tags, &mut tl);
            let tl: Vec<Item<'a>> = tl.into_iter().map(|it| if self.place(&it).is_some() { Item::NoMatrix(it.tag().unwrap()) } else { it }).collect();
            let saved = std::mem::take(&mut self.masked);
            let saved_morphs = std::mem::take(&mut self.multi_morphs);
            self.convert_timelines(None, id as i32, Some(""), &tl, Some(&name), &mut sym, None);
            self.masked = saved;
            self.multi_morphs = saved_morphs;
            sym.end();
            let file = format!("{name}.xml");
            self.files.library.insert(file.clone(), pretty(sym.as_str()).into_bytes());
            self.include(w, &file, &item_id, Some("1"));
        }
    }

    fn max_depth(&self, items: &[Item<'a>]) -> i32 {
        let mut m = 0;
        for it in items {
            if let Some(p) = self.place(it) {
                m = m.max(p.depth).max(p.clip_depth);
            }
        }
        m
    }
}

/// `DefineSceneAndFrameLabelData`: (offset, name) of scenes, then of frame labels.
pub fn scene_data(t: &Tag) -> (Vec<(u32, String)>, Vec<(u32, String)>) {
    let mut r = Reader::new(&t.body);
    let read = |r: &mut Reader| {
        let n = r.encoded_u32().unwrap_or(0);
        let mut v = Vec::new();
        for _ in 0..n {
            let (Ok(o), Ok(s)) = (r.encoded_u32(), r.string()) else { break };
            v.push((o, s));
        }
        v
    };
    let scenes = read(&mut r);
    let labels = read(&mut r);
    (scenes, labels)
}

/// A colour transform with or without alpha, as the instance carries it.
#[derive(Debug, Clone, Copy)]
pub enum Cxf {
    Alpha(model::Cx),
}

/// The state of one placed instance (`convertSymbolInstance`'s arguments).
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub name: Option<String>,
    pub matrix: Option<JMatrix>,
    pub cxform: Option<Cxf>,
    pub cache: bool,
    pub blend: i32,
    pub filters: Option<Vec<crate::filters::Filter>>,
    pub visible: bool,
    pub background: Option<crate::types::Rgba>,
    pub clip_actions: Option<usize>,
    pub metadata: Option<Rc<(crate::amf::Amf, crate::amf::Value)>>,
}

const BLEND_MODES: [Option<&str>; 15] = [None, None, Some("layer"), Some("multiply"), Some("screen"), Some("lighten"), Some("darken"), Some("difference"), Some("add"), Some("subtract"), Some("invert"), Some("alpha"), Some("erase"), Some("overlay"), Some("hardligh")];

impl<'a> Converter<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn convert_symbol_instance(&mut self, frame: i32, acc: &super::scripts::Accessibility, inst: &Instance, id: u16, w: &mut XmlWriter, small: bool, button_cx: Option<model::Cx>) {
        let mut matrix = inst.matrix.unwrap_or_default();
        if small {
            matrix = DMatrix::of(&matrix).concatenate(&DMatrix::scale_xy(1.0 / 20.0, 1.0 / 20.0)).to_jmatrix();
        }
        let mut cx = inst.cxform;
        if let Some(b) = button_cx {
            cx = Some(Cxf::Alpha(b));
        }
        let tag = self.char_tag(id);
        let name = self.symbol_name(id, "Symbol");
        w.start_with("DOMSymbolInstance", &[("libraryItemName", &name)]);
        if let Some(n) = &inst.name {
            w.attr("name", n);
            let a = acc.attributes(n, frame + 1);
            if !a.is_empty() {
                w.attr("hasAccessibleData", "true");
                for (k, v) in &a {
                    w.attr(k, v);
                }
            }
        }
        if let Some(Some(b)) = BLEND_MODES.get(inst.blend.max(0) as usize) {
            w.attr("blendMode", b);
        }
        let code = tag.map_or(0, |t| t.code);
        if morph::is_morph(code) || shape::is_shape(code) {
            w.attr("symbolType", "graphic");
            w.attr("loop", "loop");
        } else if code == DEFINE_SPRITE {
            let rect = self.ex_rect(id);
            w.attr_d("centerPoint3DX", (matrix.tx + (rect.x_max - rect.x_min) / 2) as f64 / 20.0);
            w.attr_d("centerPoint3DY", (matrix.ty + (rect.y_max - rect.y_min) / 2) as f64 / 20.0);
        } else if code == DEFINE_BUTTON || code == DEFINE_BUTTON2 {
            w.attr("symbolType", "button");
        }
        if inst.cache {
            w.attr_bool("cacheAsBitmap", true);
        }
        if !inst.visible && self.version >= FlaVersion::Cs55 {
            w.attr_bool("isVisible", false);
        }
        if inst.background.is_some() {
            w.attr_bool("bits32", false);
        }
        w.start("matrix");
        convert_matrix(&matrix, w);
        w.end();
        w.start("transformationPoint");
        w.empty("Point");
        w.end();
        if let Some(bg) = &inst.background {
            w.start_with("MatteColor", &[("color", &hex(bg))]);
            if bg.a != 255 {
                w.attr("alpha", &double_str((bg.a as f32 / 255.0) as f64));
            }
            w.end();
        }
        if let Some(Cxf::Alpha(c)) = cx {
            w.start("color");
            w.start("Color");
            let names = ["redMultiplier", "greenMultiplier", "blueMultiplier", "alphaMultiplier"];
            for (i, n) in names.iter().enumerate() {
                if c.mult[i] != 256 {
                    w.attr_f(n, c.mult[i] as f32 / 256.0);
                }
            }
            let names = ["redOffset", "greenOffset", "blueOffset", "alphaOffset"];
            for (i, n) in names.iter().enumerate() {
                if c.add[i] != 0 {
                    w.attr_i(n, c.add[i] as i64);
                }
            }
            w.end();
            w.end();
        }
        if let Some(fl) = &inst.filters {
            w.start("filters");
            for f in fl {
                super::text::convert_filter(f, w);
            }
            w.end();
        }
        if code == DEFINE_BUTTON {
            let script = self.button_script(id);
            w.start("Actionscript");
            w.start("script");
            w.cdata(&format!("on(press){{\r\n{script}}}"));
            w.end();
            w.end();
        }
        if code == DEFINE_BUTTON2 {
            let s = self.button2_scripts(id);
            if let Some(s) = s {
                w.start("Actionscript");
                w.start("script");
                w.cdata(&s);
                w.end();
                w.end();
            }
        }
        if let Some(key) = inst.clip_actions {
            w.start("Actionscript");
            w.start("script");
            let s = self.clip_action_scripts(key);
            w.cdata(&s);
            w.end();
            w.end();
        }
        if let Some(md) = &inst.metadata {
            super::scripts::persistent_data(&md.0, &md.1, w);
        }
        w.end();
    }

    pub fn ex_rect(&self, id: u16) -> crate::types::Rect {
        self.java_rect(id)
    }

    // ---- timelines ----

    /// `convertTimelines`; `sprite` is the DefineSprite, `symbol_id` may be a virtual symbol.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_timelines(&mut self, sprite: Option<u16>, symbol_id: i32, linkage: Option<&str>, items: &[Item<'a>], sprite_name: Option<&str>, w: &mut XmlWriter, script_pack: Option<&super::scripts::Pack>) {
        let mut char_pack = sprite.and_then(|id| self.character_pack(id));
        if sprite.is_none() && symbol_id == -1 {
            char_pack = self.document_pack();
        }
        let acc = match &char_pack {
            Some(p) => self.accessibility(p),
            None => super::scripts::Accessibility::default(),
        };
        let symbol_name = sprite.map(|id| self.symbol_name(id, "Symbol")).unwrap_or_else(|| "null".into());
        let (init_script, class_name) = match sprite {
            Some(id) => self.init_clip_script(id, linkage),
            None => (String::new(), None),
        };
        if let Some(cl) = class_name {
            w.attr("linkageClassName", &cl);
        }
        let root = sprite.is_none() && symbol_id == -1;
        w.start(if root { "timelines" } else { "timeline" });
        let mut scenes: Vec<(i32, i32, String, Vec<Item<'a>>)> = Vec::new();
        let mut fc = 0;
        let mut last_show = true;
        let mut scene_tag: Option<&Tag> = None;
        for it in items {
            if it.code() == DEFINE_SCENE_AND_FRAME_LABEL_DATA {
                scene_tag = it.tag();
            }
            if it.code() == SHOW_FRAME {
                last_show = true;
                fc += 1;
            } else {
                last_show = false;
            }
        }
        if !last_show {
            fc += 1;
        }
        if root {
            if let Some(st) = scene_tag {
                let (sc, _) = scene_data(st);
                for (i, (off, name)) in sc.iter().enumerate() {
                    let end = if i + 1 == sc.len() { fc - 1 } else { sc[i + 1].0 as i32 - 1 };
                    scenes.push((*off as i32, end, name.clone(), Vec::new()));
                }
                if !scenes.is_empty() {
                    let mut scene_id = 0;
                    let mut f = 0;
                    let mut cur: Vec<Item<'a>> = Vec::new();
                    let mut head: Option<Item<'a>> = None;
                    let mut head_this: Option<Item<'a>> = None;
                    for it in items {
                        cur.push(*it);
                        if matches!(it.code(), SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2) {
                            head_this = Some(*it);
                            head = head_this;
                        }
                        if it.code() == SHOW_FRAME {
                            f += 1;
                            if scene_id < scenes.len() && f > scenes[scene_id].1 {
                                if let (Some(h), None) = (head, head_this) {
                                    cur.insert(0, h);
                                }
                                scenes[scene_id].3 = std::mem::take(&mut cur);
                                scene_id += 1;
                                head_this = None;
                            }
                        }
                    }
                    if !cur.is_empty() && scene_id < scenes.len() {
                        if let (Some(h), None) = (head, head_this) {
                            cur.insert(0, h);
                        }
                        scenes[scene_id].3 = cur;
                    }
                }
            }
            if scenes.is_empty() {
                scenes.push((0, fc - 1, "Scene 1".into(), items.to_vec()));
            }
        } else {
            let name = sprite_name.unwrap_or("");
            let short = name.rsplit('/').next().unwrap_or(name).to_string();
            scenes.push((0, fc - 1, short, items.to_vec()));
        }
        for (scene_start, _, scene_name, scene_items) in scenes {
            w.start_with("DOMTimeline", &[("name", &scene_name)]);
            w.start("layers");
            let label_layers = self.convert_labels_layers(&scene_items, w);
            let has_script = self.convert_script_layer(scene_start, &init_script, &scene_items, w, script_pack);
            let mut index = label_layers + has_script as i32;
            self.convert_layers(&scene_items, &acc, &symbol_name, &mut index, w);
            self.convert_sound_layer(scene_start, &scene_items, w);
            w.end();
            w.end();
        }
        w.end();
    }

    fn convert_layers(&mut self, items: &[Item<'a>], acc: &super::scripts::Accessibility, symbol_name: &str, index: &mut i32, w: &mut XmlWriter) {
        let max_depth = self.max_depth(items);
        let mut splitters: Vec<i32> = Vec::new();
        let mut clip_places: Vec<Option<Item<'a>>> = Vec::new();
        let mut f = 0;
        let mut depth_to_clip: HashMap<i32, usize> = HashMap::new();
        let mut finish: HashMap<usize, i32> = HashMap::new();
        let mut start: HashMap<usize, i32> = HashMap::new();
        let mut occupied: HashSet<i32> = HashSet::new();
        let mut last: Option<Item<'a>> = None;
        let mut current: Vec<Item<'a>> = Vec::new();
        let sort = |c: &Converter<'a>, v: &mut Vec<Item<'a>>| v.sort_by(|a, b| c.place(b).unwrap().clip_depth.cmp(&c.place(a).unwrap().clip_depth));
        for it in items {
            if it.code() == SHOW_FRAME {
                sort(self, &mut current);
                clip_places.extend(current.drain(..).map(Some));
                f += 1;
            }
            if let Some(po) = self.place(it) {
                if po.is_move == occupied.contains(&po.depth) {
                    if !po.is_move || po.char_id != -1 || po.class_name.is_some() {
                        occupied.insert(po.depth);
                    }
                    if po.clip_depth > -1 {
                        splitters.push(f);
                        start.insert(it.key(), f);
                        current.push(*it);
                        if let Some(&k) = depth_to_clip.get(&po.depth) {
                            finish.insert(k, f - 1);
                        }
                        depth_to_clip.insert(po.depth, it.key());
                    } else if !po.is_move {
                        if let Some(k) = depth_to_clip.remove(&po.depth) {
                            finish.insert(k, f - 1);
                        }
                    }
                }
            }
            if let Some(d) = it.tag().and_then(model::remove_depth) {
                if let Some(k) = depth_to_clip.remove(&d) {
                    finish.insert(k, f - 1);
                }
                occupied.remove(&d);
            }
            last = Some(*it);
        }
        sort(self, &mut current);
        clip_places.extend(current.drain(..).map(Some));
        if last.is_some_and(|l| l.code() != SHOW_FRAME) {
            f += 1;
        }
        let frame_count = f;
        for &k in depth_to_clip.values() {
            finish.insert(k, frame_count - 1);
        }
        if splitters.first() != Some(&0) {
            splitters.insert(0, 0);
            clip_places.insert(0, None);
        }
        splitters.push(frame_count);
        clip_places.push(None);
        let mut frames_of: HashMap<i32, Vec<i32>> = HashMap::new();
        for d in (0..=max_depth).rev() {
            frames_of.insert(d, (0..frame_count).collect());
        }
        let n = clip_places.len() - 1;
        let fin = |k: usize| finish.get(&k).copied().unwrap_or(frame_count - 1);
        let mut f2d: BTreeMap<i32, BTreeMap<i32, Vec<Item<'a>>>> = BTreeMap::new();
        for fr in 0..frame_count {
            for d in 0..max_depth {
                for po_it in clip_places[..n].iter().flatten() {
                    let po = self.place(po_it).unwrap();
                    if fr >= start[&po_it.key()] && fr <= fin(po_it.key()) && d >= po.depth && d <= po.clip_depth {
                        f2d.entry(fr).or_default().entry(d).or_default().push(*po_it);
                    }
                }
            }
        }
        let mut multi: HashSet<usize> = HashSet::new();
        for depths in f2d.values() {
            for places in depths.values() {
                if places.len() > 1 {
                    for p in &places[1..] {
                        multi.insert(p.key());
                    }
                }
            }
        }
        let mut handled: HashSet<usize> = HashSet::new();
        for d in (0..=max_depth).rev() {
            for p in 0..n {
                let Some(po_it) = clip_places[p] else { continue };
                if handled.contains(&po_it.key()) {
                    continue;
                }
                let po = self.place(&po_it).unwrap();
                if po.clip_depth != d {
                    continue;
                }
                let clip_frame = splitters[p];
                let next_frame = fin(po_it.key());
                handled.insert(po_it.key());
                let mut last_frame = next_frame;
                for p2 in 0..n {
                    let Some(po2_it) = clip_places[p2] else { continue };
                    let po2 = self.place(&po2_it).unwrap();
                    let cf2 = splitters[p2];
                    let nf2 = fin(po2_it.key());
                    if last_frame + 1 == cf2 && po.depth == po2.depth && po.clip_depth == po2.clip_depth && !multi.contains(&po2_it.key()) {
                        last_frame = nf2;
                        handled.insert(po2_it.key());
                    }
                }
                let color = self.random_color();
                w.start_with("DOMLayer", &[("name", &format!("Layer {}", *index + 1)), ("color", &color), ("layerType", "mask"), ("locked", "true")]);
                let only = frames_of.get(&po.depth).cloned();
                self.convert_frames(acc, symbol_name, only.as_deref(), clip_frame, last_frame, "", "", items, po.depth, w);
                w.end();
                let parent = *index;
                *index += 1;
                for fx in clip_frame..=last_frame {
                    let mut nd = po.clip_depth - 1;
                    while nd > po.depth {
                        if !frames_of.get(&nd).is_some_and(|v| v.contains(&fx)) {
                            nd -= 1;
                            continue;
                        }
                        let clips = f2d.get(&fx).and_then(|m| m.get(&nd)).cloned();
                        if let Some(clips) = clips {
                            if clips.len() > 1 {
                                let po2_it = clips[1];
                                if handled.contains(&po2_it.key()) {
                                    nd -= 1;
                                    continue;
                                }
                                handled.insert(po2_it.key());
                                let po2 = self.place(&po2_it).unwrap();
                                let mut ndx = po.clip_depth - 1;
                                while ndx > po2.clip_depth {
                                    let only = frames_of.get(&ndx).cloned();
                                    let ne = self.write_layer(acc, symbol_name, *index, only.as_deref(), ndx, clip_frame, last_frame, parent, w, items);
                                    if let Some(v) = frames_of.get_mut(&ndx) {
                                        v.retain(|x| *x < clip_frame || *x > last_frame);
                                    }
                                    if ne {
                                        *index += 1;
                                    }
                                    ndx -= 1;
                                }
                                if let Some(mlc) = self.masked.get(&po2_it.key()).map(|m| (m.symbol, m.num_frames)) {
                                    let color = self.random_color();
                                    w.start_with("DOMLayer", &[("name", &format!("Layer {}", *index + 1)), ("color", &color), ("parentLayerIndex", &parent.to_string()), ("locked", "true")]);
                                    w.start("frames");
                                    let mut cf2 = 0;
                                    for p2 in 0..n {
                                        if clip_places[p2].is_some_and(|x| x.key() == po2_it.key()) {
                                            cf2 = splitters[p2];
                                        }
                                    }
                                    if cf2 > 0 {
                                        w.start_with("DOMFrame", &[("index", "0"), ("duration", &cf2.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                                        w.empty("elements");
                                        w.end();
                                    }
                                    w.start_with("DOMFrame", &[("index", &cf2.to_string()), ("duration", &mlc.1.to_string()), ("keyMode", &KEY_MODE_NORMAL.to_string())]);
                                    w.start("elements");
                                    w.start_with("DOMSymbolInstance", &[("libraryItemName", &format!("Symbol {}", mlc.0)), ("symbolType", "graphic"), ("loop", "loop")]);
                                    w.start("matrix");
                                    convert_matrix(&JMatrix::default(), w);
                                    w.end();
                                    w.start("transformationPoint");
                                    w.empty("Point");
                                    w.end();
                                    w.end();
                                    w.end();
                                    w.end();
                                    w.end();
                                    w.end();
                                    *index += 1;
                                    for nd2 in po2.depth..=po2.clip_depth {
                                        if let Some(v) = frames_of.get_mut(&nd2) {
                                            v.retain(|x| *x < cf2 || *x >= cf2 + mlc.1);
                                        }
                                    }
                                }
                            }
                        }
                        nd -= 1;
                    }
                }
                let mut nd = po.clip_depth - 1;
                while nd > po.depth {
                    let only = frames_of.get(&nd).cloned();
                    let ne = self.write_layer(acc, symbol_name, *index, only.as_deref(), nd, clip_frame, last_frame, parent, w, items);
                    if let Some(v) = frames_of.get_mut(&nd) {
                        v.retain(|x| *x < clip_frame || *x > last_frame);
                    }
                    if ne {
                        *index += 1;
                    }
                    nd -= 1;
                }
                if *index == parent + 1 {
                    let color = self.random_color();
                    w.start_with("DOMLayer", &[("name", &format!("Layer {}", *index + 1)), ("color", &color), ("parentLayerIndex", &parent.to_string()), ("locked", "true")]);
                    w.start("frames");
                    w.start("DOMFrame");
                    w.attr_i("index", 0);
                    w.attr_i("duration", last_frame as i64 + 1);
                    w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
                    w.start("elements");
                    w.end();
                    w.end();
                    w.end();
                    w.end();
                    *index += 1;
                }
                if let Some(v) = frames_of.get_mut(&po.depth) {
                    v.retain(|x| *x < clip_frame || *x > last_frame);
                }
            }
            let only = frames_of.get(&d).cloned();
            let ne = self.write_layer(acc, symbol_name, *index, only.as_deref(), d, 0, i32::MAX, -1, w, items);
            if ne {
                *index += 1;
            }
        }
        if *index == 0 {
            let color = self.random_color();
            w.start_with("DOMLayer", &[("name", "Layer 1"), ("color", &color)]);
            w.attr_bool("current", true);
            w.attr_bool("isSelected", true);
            w.start("frames");
            w.start("DOMFrame");
            w.attr_i("index", 0);
            w.attr_i("duration", frame_count as i64);
            w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
            w.start("elements");
            w.end();
            w.end();
            w.end();
            w.end();
            *index += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_layer(&mut self, acc: &super::scripts::Accessibility, symbol_name: &str, index: i32, only: Option<&[i32]>, d: i32, start: i32, end: i32, parent: i32, w: &mut XmlWriter, items: &[Item<'a>]) -> bool {
        let mut prev = XmlWriter::new();
        let color = self.random_color();
        prev.start_with("DOMLayer", &[("name", &format!("Layer {}", index + 1)), ("color", &color)]);
        if d == 1 {
            prev.attr_bool("current", true);
            prev.attr_bool("isSelected", true);
        }
        if parent != -1 {
            prev.attr_i("parentLayerIndex", parent as i64);
            prev.attr_bool("locked", true);
        }
        prev.close();
        let before = w.len();
        self.convert_frames(acc, symbol_name, only, start, end, prev.as_str(), "</DOMLayer>", items, d, w);
        w.len() != before
    }

    /// `convertFrame`.
    #[allow(clippy::too_many_arguments)]
    pub fn convert_frame(&mut self, shape_tween: bool, stream: Option<&StreamRange>, start_sound: Option<(&'a Tag, &model::SoundInfo)>, frame: i32, duration: i32, script: &str, elements: &str, w: &mut XmlWriter, acceleration: Option<i32>) {
        w.start("DOMFrame");
        w.attr_i("index", frame as i64);
        if duration > 1 {
            w.attr_i("duration", duration as i64);
        }
        if shape_tween {
            w.attr("tweenType", "shape");
            w.attr_i("keyMode", KEY_MODE_SHAPE_TWEEN as i64);
            if let Some(a) = acceleration {
                w.attr_i("acceleration", a as i64);
            }
        } else {
            w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
        }
        if let (Some(r), None) = (stream, start_sound) {
            let name = format!("{}.{}", r.fla_name(), self.stream_ext(r));
            w.attr("soundName", &name);
            w.attr("soundSync", "stream");
            w.start("SoundEnvelope");
            w.empty_with("SoundEnvelopePoint", &[("level0", "32768"), ("level1", "32768")]);
            w.end();
        }
        if let Some((st, info)) = start_sound {
            self.convert_sound_usage(w, st, info);
        }
        if !script.is_empty() {
            w.start("Actionscript");
            w.start("script");
            w.cdata(script);
            w.end();
            w.end();
        }
        w.start("elements");
        w.raw(elements);
        w.end();
        w.end();
    }

    /// `convertFrames` for the objects at `depth`.
    #[allow(clippy::too_many_arguments)]
    fn convert_frames(&mut self, acc: &super::scripts::Accessibility, _symbol_name: &str, only: Option<&[i32]>, start_frame: i32, end_frame: i32, prev: &str, after: &str, items: &[Item<'a>], depth: i32, w: &mut XmlWriter) {
        let mut last_in = false;
        let mut w2 = XmlWriter::new();
        let mut prev = format!("{prev}<frames>");
        let mut frame = -1;
        let mut last_elements = String::new();
        let mut last_char: Option<u16> = None;
        let mut last_matrix: Option<JMatrix> = None;
        let mut duration = 1;
        let mut character: Option<u16> = None;
        let mut inst = Instance { visible: true, filters: Some(Vec::new()), ..Default::default() };
        let mut ratio = -1;
        let mut shape_tween = false;
        let mut tweener: Option<u16> = None;
        let mut ratios: Vec<i32> = Vec::new();
        let mut starts: HashSet<i32> = HashSet::new();
        let mut reg_points: HashMap<String, [f64; 2]> = HashMap::new();
        let mut standalone: Option<(u16, Option<JMatrix>)> = None;
        let mut tl: Vec<Item<'a>> = items.to_vec();
        for it in items.iter().rev() {
            if it.code() == SHOW_FRAME {
                break;
            }
            if self.place(it).is_some() {
                tl.push(Item::Frame);
                break;
            }
        }
        for it in &tl {
            if let Some(po) = self.place(it) {
                if po.depth == depth {
                    if !po.is_move {
                        starts.insert(frame + 1);
                    }
                    let new_char: Option<u16> = if po.char_id != -1 {
                        self.char_tag(po.char_id as u16).map(|_| po.char_id as u16)
                    } else if let Some(cl) = &po.class_name {
                        self.char_by_class(cl)
                    } else {
                        None
                    };
                    if new_char.is_some() && new_char != character && shape_tween && character.is_some() {
                        tweener = character;
                        shape_tween = false;
                    }
                    let new_char = if po.char_id == -1 && po.class_name.is_none() { character } else { new_char };
                    character = new_char;
                    if character.is_some() {
                        if po.is_move {
                            if let Some(a) = &po.amf {
                                inst.metadata = Some(Rc::new(a.clone()));
                            }
                            if let Some(m) = po.matrix {
                                inst.matrix = Some(m);
                            }
                            if let Some(n) = &po.name {
                                inst.name = Some(n.clone());
                            }
                            if let Some(c) = po.cxform {
                                inst.cxform = Some(Cxf::Alpha(c));
                            }
                            if po.has_clip_actions {
                                inst.clip_actions = Some(it.key());
                            }
                            if po.cache_as_bitmap {
                                inst.cache = true;
                            }
                            if po.blend > 0 {
                                inst.blend = po.blend;
                            }
                            if let Some(f) = &po.filters {
                                inst.filters = Some(f.clone());
                            }
                            if po.ratio > -1 {
                                ratio = po.ratio;
                            }
                            if po.num >= 3 {
                                if po.visible.is_some() {
                                    inst.visible = po.is_visible();
                                }
                            } else {
                                inst.visible = true;
                            }
                            if po.background.is_some() {
                                inst.background = po.background;
                            }
                        } else {
                            inst.metadata = po.amf.clone().map(Rc::new);
                            inst.matrix = po.matrix;
                            inst.name = po.name.clone();
                            inst.cxform = po.cxform.map(Cxf::Alpha);
                            inst.cache = po.cache_as_bitmap;
                            inst.blend = po.blend;
                            inst.filters = po.filters.clone();
                            ratio = po.ratio;
                            inst.clip_actions = if po.has_clip_actions { Some(it.key()) } else { None };
                            inst.visible = po.is_visible();
                            inst.background = po.background;
                        }
                    }
                }
            }
            if it.tag().and_then(model::remove_depth) == Some(depth) {
                starts.insert(frame + 1);
                if shape_tween && character.is_some() {
                    tweener = character;
                    shape_tween = false;
                }
                character = None;
                inst = Instance { visible: true, filters: Some(Vec::new()), ..Default::default() };
            }
            if it.code() != SHOW_FRAME {
                continue;
            }
            frame += 1;
            if frame < start_frame || frame > end_frame || only.is_some_and(|o| !o.contains(&frame)) {
                if last_in {
                    if !last_elements.is_empty() {
                        self.convert_frame(false, None, None, frame - duration, duration, "", &last_elements, &mut w2, None);
                        duration = 1;
                    } else {
                        duration += 1;
                    }
                    last_elements.clear();
                    last_in = false;
                    last_char = None;
                    last_matrix = None;
                } else if frame == 0 {
                    duration = 1;
                } else {
                    duration += 1;
                }
                continue;
            }
            last_in = true;
            let mut ew = XmlWriter::new();
            if let Some(m) = tweener {
                let mut add_last = XmlWriter::new();
                if let Some(ch) = character.filter(|&c| self.is_morph(c) && !self.multi_morphs.contains(&c)) {
                    if let Ok(ms) = MorphShape::read(self.char_tag(ch).unwrap()) {
                        let num = if ms.version == 1 { 3 } else { 4 };
                        self.convert_shape(inst.matrix.as_ref(), num, &ms.start_shape(), true, false, &mut add_last, false);
                    }
                    shape_tween = true;
                } else if let Ok(ms) = MorphShape::read(self.char_tag(m).unwrap()) {
                    let num = if ms.version == 1 { 3 } else { 4 };
                    let end = ms.shape_at(65535);
                    self.convert_shape(inst.matrix.as_ref(), num, &end, true, false, &mut add_last, false);
                }
                let ease = super::tween::ease_from_shape_ratios(&mut ratios);
                self.convert_frame(true, None, None, frame - duration, duration, "", &last_elements, &mut w2, ease.map(|e| -e));
                duration = 1;
                last_elements = add_last.to_string();
                last_matrix = inst.matrix;
                last_char = character;
                tweener = None;
                ratios.clear();
                continue;
            }
            let ch_code = character.and_then(|c| self.char_tag(c)).map(|t| t.code);
            let is_shape = ch_code.is_some_and(shape::is_shape);
            if is_shape && standalone.is_some() {
                let (sid, sm) = standalone.take().unwrap();
                let i2 = Instance { matrix: sm, ..inst.clone() };
                self.convert_symbol_instance(frame, acc, &i2, sid, &mut ew, false, None);
            } else if is_shape && self.non_library.contains(&character.unwrap()) {
                if last_char == character && inst.matrix == last_matrix {
                    ew.raw(&last_elements);
                } else if let Ok(s) = ShapeTag::read(self.char_tag(character.unwrap()).unwrap()) {
                    self.convert_shape(inst.matrix.as_ref(), s.shape_num, &s.shape, false, false, &mut ew, false);
                }
                shape_tween = false;
                tweener = None;
            } else if ch_code.is_some_and(morph::is_morph) {
                let m = character.unwrap();
                if self.multi_morphs.contains(&m) {
                    shape_tween = false;
                    tweener = None;
                    standalone = Some((m, inst.matrix));
                    self.convert_symbol_instance(frame, acc, &inst, m, &mut ew, false, None);
                } else {
                    ratios.push(if ratio == -1 { 0 } else { ratio });
                    if last_char == Some(m) && inst.matrix == last_matrix {
                        ew.raw(&last_elements);
                    } else if let Ok(ms) = MorphShape::read(self.char_tag(m).unwrap()) {
                        let num = if ms.version == 1 { 3 } else { 4 };
                        self.convert_shape(inst.matrix.as_ref(), num, &ms.start_shape(), true, false, &mut ew, false);
                    }
                    shape_tween = true;
                }
            } else {
                shape_tween = false;
                match ch_code {
                    Some(DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT) => {
                        let t = self.char_tag(character.unwrap()).unwrap();
                        let m = inst.matrix.unwrap_or_default();
                        self.convert_text(frame, acc, inst.name.as_deref(), t, &m, inst.filters.as_deref(), &mut ew);
                    }
                    Some(DEFINE_VIDEO_STREAM) => {
                        let t = self.char_tag(character.unwrap()).unwrap();
                        self.convert_video_instance(inst.name.as_deref(), &inst.matrix.unwrap_or_default(), t, &mut ew);
                    }
                    Some(code) if crate::export::image::is_image(code) => {
                        self.convert_image_instance(inst.name.as_deref(), &inst.matrix.unwrap_or_default(), character.unwrap(), &mut ew);
                    }
                    Some(_) => {
                        let id = character.unwrap();
                        let small = self.small.contains(&id);
                        self.convert_symbol_instance(frame, acc, &inst, id, &mut ew, small, None);
                    }
                    None => {}
                }
            }
            let elements = ew.to_string();
            if self.version >= FlaVersion::Cs5 && elements.contains("<DOMSymbolInstance") {
                if let Some(id) = character {
                    if self.is_bounded(id) {
                        let item = self.symbol_name(id, "Symbol");
                        if !reg_points.contains_key(&item) {
                            let b = self.ex_rect(id);
                            let (w_, h_) = ((b.x_max - b.x_min) as f64, (b.y_max - b.y_min) as f64);
                            reg_points.insert(item, [if w_ == 0.0 { 0.0 } else { -b.x_min as f64 / w_ }, if h_ == 0.0 { 0.0 } else { -b.y_min as f64 / h_ }]);
                        }
                    }
                }
            }
            if (elements != last_elements || starts.contains(&frame)) && frame > 0 {
                self.convert_frame(false, None, None, frame - duration, duration, "", &last_elements, &mut w2, None);
                duration = 1;
            } else if frame == 0 {
                duration = 1;
            } else {
                duration += 1;
            }
            last_elements = elements;
            last_char = character;
            last_matrix = inst.matrix;
        }
        if (!last_elements.is_empty() || !w2.is_empty()) && last_in && frame >= start_frame && frame <= end_frame && only.is_none_or(|o| o.contains(&frame)) {
            frame += 1;
            self.convert_frame(false, None, None, frame - duration, duration, "", &last_elements, &mut w2, None);
        }
        let after = format!("</frames>{after}");
        if !w2.is_empty() {
            let fr = if self.version >= FlaVersion::Cs5 { self.swf.frame_rate as f64 } else { 0.0 };
            let frames_xml = super::tween::detect(w2.as_str(), &starts, fr, &reg_points);
            if frames_xml.contains("tweenType=\"motion object\"") {
                if prev.contains("<DOMLayer") {
                    prev = prev.replacen("<DOMLayer", "<DOMLayer animationType=\"motion object\"", 1);
                } else {
                    w.attr("animationType", "motion object");
                }
            }
            w.raw(&prev);
            w.raw(&frames_xml);
            w.raw(&after);
        }
    }

    fn is_bounded(&self, id: u16) -> bool {
        self.char_tag(id).is_some_and(|t| {
            shape::is_shape(t.code) || morph::is_morph(t.code) || matches!(t.code, DEFINE_SPRITE | DEFINE_BUTTON | DEFINE_BUTTON2 | DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT | DEFINE_VIDEO_STREAM) || crate::export::image::is_image(t.code)
        })
    }

    pub fn char_by_class(&self, class: &str) -> Option<u16> {
        self.classes.iter().find(|(_, c)| c.as_str() == class).map(|(&id, _)| id)
    }

    fn convert_labels_layers(&mut self, items: &[Item<'a>], w: &mut XmlWriter) -> i32 {
        let mut labels: HashMap<i32, Vec<(String, bool)>> = HashMap::new();
        let mut frame = 0;
        let mut layers = 0;
        for it in items {
            if it.code() == FRAME_LABEL {
                let t = it.tag().unwrap();
                let mut r = Reader::new(&t.body);
                let name = r.string().unwrap_or_default();
                let anchor = r.u8().is_ok_and(|b| b == 1);
                let v = labels.entry(frame).or_default();
                v.push((name, anchor));
                layers = layers.max(v.len() as i32);
            } else if it.code() == SHOW_FRAME {
                frame += 1;
            }
        }
        let frame_count = frame;
        for lay in 0..layers {
            let color = self.random_color();
            let name = if layers > 1 { format!("Labels Layer {}", lay + 1) } else { "Labels Layer".into() };
            w.start_with("DOMLayer", &[("name", &name), ("color", &color)]);
            w.start("frames");
            let mut duration = 0;
            for i in 0..frame_count {
                let label = labels.get(&i).and_then(|v| v.get(lay as usize));
                match label {
                    None => duration += 1,
                    Some((name, anchor)) => {
                        if duration > 0 {
                            w.start_with("DOMFrame", &[("index", &(i - duration).to_string())]);
                            if duration > 1 {
                                w.attr_i("duration", duration as i64);
                            }
                            w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
                            w.value("elements", "");
                            w.end();
                        }
                        w.start_with("DOMFrame", &[("index", &i.to_string())]);
                        w.attr_i("keyMode", KEY_MODE_NORMAL as i64);
                        w.attr("name", name);
                        if *anchor {
                            w.attr("labelType", "anchor");
                            w.attr_bool("bookmark", true);
                        } else {
                            w.attr("labelType", "name");
                        }
                        w.value("elements", "");
                        w.end();
                        duration = 0;
                    }
                }
            }
            w.end();
            w.end();
        }
        layers
    }

    pub fn random_outline(&mut self) -> String {
        self.random_color()
    }

    fn publish_settings(&self, base: &str, width: f64, height: f64, network: bool, doc_pack: &Option<super::scripts::Pack>) -> String {
        super::media::publish_settings(self, base, width, height, network, doc_pack)
    }
}

const RECT_EMPTY: crate::types::Rect = crate::types::Rect { x_min: i32::MAX, x_max: i32::MIN, y_min: i32::MAX, y_max: i32::MIN };

fn union(a: &mut crate::types::Rect, b: &crate::types::Rect) {
    a.x_min = a.x_min.min(b.x_min);
    a.y_min = a.y_min.min(b.y_min);
    a.x_max = a.x_max.max(b.x_max);
    a.y_max = a.y_max.max(b.y_max);
}

fn filter_delta(f: &crate::filters::Filter) -> (f64, f64) {
    use crate::filters::Filter as F;
    match f {
        F::DropShadow(s) => (s.blur_x + (s.distance * s.angle.cos()).abs(), s.blur_y + (s.distance * s.angle.sin()).abs()),
        F::Bevel(b) | F::GradientBevel(b) | F::GradientGlow(b) => (b.blur_x + (b.distance * b.angle.cos()).abs(), b.blur_y + (b.distance * b.angle.sin()).abs()),
        F::Blur { blur_x, blur_y, .. } => (*blur_x, *blur_y),
        F::Glow(g) => (g.blur_x, g.blur_y),
        _ => (0.0, 0.0),
    }
}

impl<'a> Converter<'a> {
    /// A character's bounds (sprites: every placement, matrices truncated).
    pub fn java_rect(&self, id: u16) -> crate::types::Rect {
        if let Some(r) = self.rect_cache.borrow().get(&id) {
            return *r;
        }
        let r = self.java_rect_in(id, &mut Vec::new()).unwrap_or_default();
        self.rect_cache.borrow_mut().insert(id, r);
        r
    }

    fn java_rect_in(&self, id: u16, added: &mut Vec<u16>) -> Option<crate::types::Rect> {
        let tag = self.char_tag(id)?;
        let mut r = Reader::new(&tag.body);
        r.u16().ok()?;
        Some(match tag.code {
            c if shape::is_shape(c) || matches!(c, DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT) => crate::types::Rect::read(&mut r).ok()?,
            c if morph::is_morph(c) => {
                let s = crate::types::Rect::read(&mut r).ok()?;
                let e = crate::types::Rect::read(&mut r).ok()?;
                crate::types::Rect { x_min: s.x_min.min(e.x_min), y_min: s.y_min.min(e.y_min), x_max: s.x_max.max(e.x_max), y_max: s.y_max.max(e.y_max) }
            }
            DEFINE_VIDEO_STREAM => {
                r.u16().ok()?;
                let w = r.u16().ok()? as i32;
                let h = r.u16().ok()? as i32;
                crate::types::Rect { x_min: 0, x_max: w * 20, y_min: 0, y_max: h * 20 }
            }
            c if crate::export::image::is_image(c) => {
                let jt = self.swf.timeline.tags.iter().find(|t| t.code == JPEG_TABLES).map(|t| &t.body[..]);
                let img = crate::export::image::Bitmap::read(tag, jt).ok()?.pixels().ok()?;
                crate::types::Rect { x_min: 0, x_max: img.width() as i32 * 20, y_min: 0, y_max: img.height() as i32 * 20 }
            }
            DEFINE_BUTTON | DEFINE_BUTTON2 => {
                let b = Button::read(tag)?;
                let mut rect = RECT_EMPTY;
                let mut found = false;
                for rec in &b.records {
                    if added.contains(&rec.char_id) || self.char_tag(rec.char_id).is_none() {
                        continue;
                    }
                    added.push(rec.char_id);
                    let r2 = self.java_rect_in(rec.char_id, added);
                    added.pop();
                    let Some(mut r2) = r2 else { continue };
                    let m = rec.matrix;
                    let ap = |x: i32, y: i32| {
                        let sx = if m.has_scale { m.scale_x } else { 1.0 };
                        let sy = if m.has_scale { m.scale_y } else { 1.0 };
                        let (r0, r1) = if m.has_rotate { (m.rotate0, m.rotate1) } else { (0.0, 0.0) };
                        ((x as f32 * sx + y as f32 * r1 + m.tx as f32) as i32, (x as f32 * r0 + y as f32 * sy + m.ty as f32) as i32)
                    };
                    let (ax, ay) = ap(r2.x_min, r2.y_min);
                    let (bx, by) = ap(r2.x_max, r2.y_max);
                    r2 = crate::types::Rect { x_min: ax.min(bx), x_max: ax.max(bx), y_min: ay.min(by), y_max: ay.max(by) };
                    let (mut dx, mut dy) = (0.0f64, 0.0f64);
                    if rec.has_filters {
                        for f in &rec.filters {
                            let (x, y) = filter_delta(f);
                            dx += x;
                            dy += y;
                        }
                    }
                    let (dx, dy) = ((dx.ceil() * 20.0) as i32, (dy.ceil() * 20.0) as i32);
                    r2.x_min -= dx;
                    r2.y_min -= dy;
                    r2.x_max += dx;
                    r2.y_max += dy;
                    union(&mut rect, &r2);
                    found = true;
                }
                if found {
                    rect
                } else {
                    crate::types::Rect::default()
                }
            }
            DEFINE_SPRITE => {
                let tags = &tag.sprite.as_ref()?.tags;
                let mut ret = RECT_EMPTY;
                let mut found = false;
                let mut depth_char: HashMap<i32, Option<u16>> = HashMap::new();
                let mut depth_matrix: HashMap<i32, Option<JMatrix>> = HashMap::new();
                for t in tags {
                    if let Some(d) = model::remove_depth(t) {
                        depth_char.remove(&d);
                        depth_matrix.remove(&d);
                        continue;
                    }
                    let Some(po) = self.place(&Item::Tag(t)) else { continue };
                    let mut m = po.matrix;
                    if m.is_none() && po.is_move {
                        m = depth_matrix.get(&po.depth).copied().flatten();
                    }
                    if po.is_move != depth_char.contains_key(&po.depth) {
                        continue;
                    }
                    depth_matrix.insert(po.depth, m);
                    let ch = if po.class_name.is_some() || po.char_id > -1 {
                        let c = match &po.class_name {
                            Some(cl) => self.char_by_class(cl),
                            None => self.char_tag(po.char_id as u16).map(|_| po.char_id as u16),
                        };
                        depth_char.insert(po.depth, c);
                        c
                    } else {
                        depth_char.get(&po.depth).copied().flatten()
                    };
                    let Some(ch) = ch else { continue };
                    if crate::export::font::is_font(self.char_tag(ch).map_or(0, |t| t.code)) {
                        continue;
                    }
                    let mut r = crate::types::Rect::default();
                    if !added.contains(&ch) {
                        added.push(ch);
                        if let Some(x) = self.java_rect_in(ch, added) {
                            if x.x_min < x.x_max && x.y_min < x.y_max {
                                r = x;
                            }
                        }
                        added.pop();
                    }
                    if let Some(m) = m {
                        let d = DMatrix::of(&m);
                        let pts = [d.transform(r.x_min as f64, r.y_min as f64), d.transform(r.x_max as f64, r.y_min as f64), d.transform(r.x_max as f64, r.y_max as f64), d.transform(r.x_min as f64, r.y_max as f64)];
                        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, -f64::MAX, -f64::MAX);
                        for (x, y) in pts {
                            x0 = x0.min(x);
                            x1 = x1.max(x);
                            y0 = y0.min(y);
                            y1 = y1.max(y);
                        }
                        r = crate::types::Rect { x_min: x0 as i32, x_max: x1 as i32, y_min: y0 as i32, y_max: y1 as i32 };
                    }
                    union(&mut ret, &r);
                    found = true;
                }
                if found {
                    ret
                } else {
                    crate::types::Rect::default()
                }
            }
            _ => return None,
        })
    }
}
