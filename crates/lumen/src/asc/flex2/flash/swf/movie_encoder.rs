//! The asset half of `flash.swf.Frame` and `flash.swf.MovieEncoder`: the symbol classes, exports and fonts a frame
//! carries, and the order `export` writes their definitions in (each tag after the tags it refers to).

use std::collections::HashMap;

use super::dictionary::TagMap;
use super::tag_encoder::TagEncoder;
use super::tags::{tag_eq, Body, TagRef};
use crate::asc::util::jhash::JHashMap;

/// `Frame`'s `symbolClass`, `exports`/`exportDefs` and `fonts`.
#[derive(Default)]
pub struct FrameAssets {
    pub symbol_class: JHashMap<String, TagRef>,
    exports: HashMap<String, TagRef>,
    export_defs: Vec<TagRef>,
    pub fonts: Vec<TagRef>,
}

impl FrameAssets {
    pub fn add_symbol_class(&mut self, class_name: &str, symbol: TagRef) {
        self.symbol_class.insert(class_name.to_string(), symbol);
    }

    pub fn has_symbol_classes(&self) -> bool {
        !self.symbol_class.is_empty()
    }

    pub fn add_export(&mut self, def: &TagRef) {
        let name = def.borrow().name.clone().unwrap_or_default();
        if let Some(old) = self.exports.insert(name, def.clone()) {
            if let Some(i) = self.export_defs.iter().position(|d| tag_eq(d, &old)) {
                self.export_defs.remove(i);
            }
        }
        self.export_defs.push(def.clone());
    }

    pub fn has_exports(&self) -> bool {
        !self.exports.is_empty()
    }

    pub fn add_font(&mut self, tag: TagRef) {
        self.fonts.push(tag);
    }

    /// `getReferences()` without control tags, which mxmlc's frames don't have.
    pub fn get_references(&self) -> Vec<TagRef> {
        let mut list: Vec<TagRef> = self.export_defs.clone();
        list.extend(self.symbol_class.values().cloned());
        list
    }
}

/// `MovieEncoder` over a `TagEncoder`.
pub struct MovieEncoder {
    pub encoder: TagEncoder,
    done: TagMap<()>,
}

impl MovieEncoder {
    pub fn new(swf_version: i32, debug: bool) -> MovieEncoder {
        MovieEncoder { encoder: TagEncoder::new(swf_version, debug), done: TagMap::default() }
    }

    /// `define(tag)`.
    pub fn define(&mut self, tag: &TagRef) {
        if self.done.contains(tag) {
            return;
        }
        let refs = tag.borrow().get_references();
        for r in &refs {
            self.define(r);
        }
        let is_import_record = matches!(tag.borrow().body, Body::ImportRecord);
        if !is_import_record {
            self.encoder.visit(tag);
            let after = {
                let t = tag.borrow();
                match &t.body {
                    Body::DefineSprite(s) => s.scaling_grid.clone(),
                    Body::DefineButton(b) => b.scaling_grid.clone(),
                    Body::DefineShape(s) => s.scaling_grid.clone(),
                    Body::DefineFont2(f) if t.code == super::tags::codes::DEFINE_FONT3 => f.zones.clone(),
                    Body::DefineEditText(e) => e.csm_text_settings.clone(),
                    Body::DefineText(x) => x.csm_text_settings.clone(),
                    _ => None,
                }
            };
            self.visit_after(after);
            let license = tag.borrow().license();
            self.visit_after(license);
        }
        self.done.put(tag, ());
    }

    fn visit_after(&mut self, tag: Option<TagRef>) {
        if let Some(t) = tag {
            self.encoder.visit(&t);
            self.done.put(&t, ());
        }
    }

    /// The part of `export` from a frame's definitions through its fonts, as SWF tag bytes.
    pub fn frame_definitions(&mut self, frame: &FrameAssets) -> Vec<u8> {
        for r in frame.get_references() {
            self.define(&r);
        }
        if frame.has_exports() {
            let tag = super::tags::Tag::new_ref(super::tags::codes::EXPORT_ASSETS, Body::ExportAssets { exports: frame.export_defs.clone() });
            self.encoder.visit(&tag);
        }
        for font in &frame.fonts {
            if !self.done.contains(font) {
                self.encoder.visit(font);
                self.done.put(font, ());
            }
        }
        self.encoder.take_bytes()
    }

    /// The frame's `SymbolClass` tag, as `export` builds it (a fresh map filled by `putAll`).
    pub fn symbol_class(&mut self, frame: &FrameAssets, top_level_class: Option<&str>) -> Vec<u8> {
        let mut classes: JHashMap<String, TagRef> = JHashMap::new();
        classes.put_all(frame.symbol_class.iter().map(|(k, v)| (k.clone(), v.clone())), frame.symbol_class.len());
        let entries: Vec<(String, TagRef)> = classes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        self.encoder.symbol_class(&entries, top_level_class);
        self.encoder.take_bytes()
    }
}
