//! `IggySwf`: the movie block. Reads the dictionary, fonts, texts, timelines, tables and ABC
//! blocks, and writes edits by appending replacements and relocating pointers to them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::model::*;
use super::stream::{err, field, write_fields, Field, Fields, Fixup, Stream};
use crate::error::Result;

#[derive(Clone, Copy, Debug)]
pub struct DictEntry {
    pub id: i32,
    pub address: u64,
    pub ty: i32,
}

impl DictEntry {
    pub fn is_null(&self) -> bool {
        self.address == 0
    }
}

#[derive(Clone, Copy)]
struct Move {
    old: u64,
    new: u64,
    len: u64,
}

fn relocated(a: u64, moves: &BTreeMap<u64, Move>) -> u64 {
    if let Some((_, m)) = moves.range(..=a).next_back() {
        if a >= m.old && a < m.old + m.len {
            return m.new + a - m.old;
        }
    }
    a
}

fn move_index(moves: &[Move]) -> BTreeMap<u64, Move> {
    moves.iter().map(|m| (m.old, *m)).collect()
}

fn import_ids(libs: &[Import]) -> HashSet<i32> {
    libs.iter().flat_map(|l| l.symbols.iter().map(|s| s.character_id as i32)).collect()
}

fn import_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('\0') {
        return err("Invalid Iggy library or import symbol name");
    }
    Ok(())
}

fn wlen(s: &str) -> u64 {
    s.encode_utf16().count() as u64
}

pub struct Serialized {
    pub data: Vec<u8>,
    pub fields: Vec<Field>,
}

pub enum Obj<'a> {
    Font(&'a mut Font),
    Text(&'a Text),
}

pub struct Swf {
    pub hdr: Header,
    pub is64: bool,
    ptr: u64,
    pub name: String,
    pub source_name: Option<String>,
    original: Vec<u8>,
    pub fonts: Vec<Font>,
    pub texts: Vec<Text>,
    font_addresses: Vec<u64>,
    text_addresses: Vec<u64>,
    font_snapshots: Vec<Vec<u8>>,
    text_snapshots: Vec<Vec<u8>>,
    pub dictionary: Vec<DictEntry>,
    frame_labels: Vec<NamedFrame>,
    scenes: Vec<NamedFrame>,
    symbol_classes: Vec<SymbolClass>,
    imports: Vec<Import>,
    import_changes: Option<Vec<Import>>,
    symbol_changes: Option<Vec<SymbolClass>>,
    scene_changes: Option<Vec<NamedFrame>>,
    frame_label_changes: Option<Vec<NamedFrame>>,
    as3_change: Option<bool>,
    scaling_grid_changes: Vec<(i32, Option<[f32; 4]>)>,
    pub timeline: Timeline,
    dictionary_ids: HashMap<u64, i32>,
    dictionary_reference_ids: Vec<(u64, i32)>,
    pub texture_resource_name: String,
    removed: BTreeSet<i32>,
    object_changes: Vec<(i32, ObjectData)>,
    texture_changes: Vec<(u32, Option<Vec<u8>>)>,
    timeline_changes: Vec<(i32, Replacement)>,
    pub sprite_timelines: Vec<(i32, Timeline)>,
    dictionary_fixup_entries: Vec<u64>,
    pub abc_blocks: Vec<DeclStrings>,
    original_abc: Vec<DeclStrings>,
    original_fields: Vec<Field>,
    symbol_class_names: Vec<(u64, String)>,
    original_text_symbols: HashSet<u32>,
}

fn put<K: PartialEq, V>(v: &mut Vec<(K, V)>, k: K, val: V) {
    match v.iter_mut().find(|e| e.0 == k) {
        Some(e) => e.1 = val,
        None => v.push((k, val)),
    }
}

fn get<'a, K: PartialEq, V>(v: &'a [(K, V)], k: &K) -> Option<&'a V> {
    v.iter().find(|e| e.0 == *k).map(|e| &e.1)
}

/// `IggySwf.has32Root`: whether a 32-bit block uses the 128-byte root.
pub fn has32_root(data: &[u8], fields: &[Field]) -> Result<bool> {
    if let Some(f) = fields.iter().find(|f| f.offset == 4) {
        return Ok(f.op == 2);
    }
    if data.len() < 128 {
        return Ok(false);
    }
    let mut s = Stream::reader(data);
    let base = s.pointer(false)?;
    s.seek(0x28)?;
    let count = s.u32()?;
    s.seek(0x40)?;
    let fps = s.f32()?;
    Ok(base >= 128 && base <= data.len() as u64 && count <= (data.len() as u64 - base) / 4 && (1.0..=1000.0).contains(&fps))
}

fn read_reference(bytes: &[u8], pos: u64, is64: bool) -> u64 {
    let n = if is64 { 8 } else { 4 };
    let mut v = 0u64;
    for i in 0..n {
        v |= (bytes[pos as usize + i] as u64) << (8 * i);
    }
    if is64 {
        v
    } else {
        v as u32 as i32 as i64 as u64
    }
}

/// `IggySwf.serialize`: an object's bytes and fixups. 32-bit objects keep the original
/// body's context and opaque pointer fields.
pub fn serialize(is64: bool, original: &[u8], original_fields: &[Field], address: Option<u64>, obj: Obj) -> Result<Serialized> {
    let mut s = Stream::indexed();
    let size;
    let mut font32 = None;
    match obj {
        Obj::Font(f) => {
            f.write(&mut s)?;
            size = f.x32.as_ref().map_or(352, |x| x.header_size as u64);
            if f.x32.is_some() {
                font32 = Some(f);
            }
        }
        Obj::Text(t) => {
            t.write(&mut s)?;
            size = 68;
        }
    }
    let data = std::mem::take(&mut s.data);
    let program = s.idx().bytes();
    let mut fields = Fixup::parse(is64, &program, data.len() as u64)?.fields;
    if let (false, Some(start)) = (is64, address) {
        let mut combined = Fields::default();
        let parts: BTreeMap<u64, u64> = font32.as_ref().map(|f| f.x32.as_ref().unwrap().relocated_parts.iter().copied().collect()).unwrap_or_default();
        for f in original_fields {
            if f.offset >= start && f.offset < start + size {
                combined.put(field(f.offset - start, f.op));
            }
            if let Some(font) = font32.as_mut() {
                if let Some((&old, &new)) = parts.range(..=f.offset).next_back() {
                    let x = font.x32.as_mut().unwrap();
                    let psize = x.part_sizes[&old];
                    if f.offset >= old && f.offset < old + psize {
                        let offset = new + f.offset - old;
                        combined.put(field(offset, f.op));
                        if f.op == 2 && psize == 16 && f.offset - old != 12 {
                            let value = read_reference(original, f.offset, false);
                            if value != 1 {
                                let t = f.offset.wrapping_add(value);
                                match x.external_references.iter_mut().find(|e| e.0 == offset) {
                                    Some(e) => e.1 = t,
                                    None => x.external_references.push((offset, t)),
                                }
                            }
                        }
                    }
                }
            }
        }
        for f in &fields {
            combined.put(*f);
        }
        fields = combined.values();
    }
    Ok(Serialized { data, fields })
}

impl Swf {
    pub fn read(data: Vec<u8>, fixups: Vec<Field>, is64: bool) -> Result<Swf> {
        let ptr = if is64 { 8 } else { 4 };
        let mut stream = Stream::reader(&data);
        if data.len() < if is64 { 184 } else { 64 } {
            return err("Iggy reference outside main block: 0");
        }
        let hdr = if is64 {
            Header::read64(&mut stream)?
        } else if has32_root(&data, &fixups)? {
            Header::read32(&mut stream)?
        } else {
            Header::read_legacy(&mut stream)?
        };
        let mut swf = Swf {
            hdr,
            is64,
            ptr,
            name: String::new(),
            source_name: None,
            original: data,
            fonts: Vec::new(),
            texts: Vec::new(),
            font_addresses: Vec::new(),
            text_addresses: Vec::new(),
            font_snapshots: Vec::new(),
            text_snapshots: Vec::new(),
            dictionary: Vec::new(),
            frame_labels: Vec::new(),
            scenes: Vec::new(),
            symbol_classes: Vec::new(),
            imports: Vec::new(),
            import_changes: None,
            symbol_changes: None,
            scene_changes: None,
            frame_label_changes: None,
            as3_change: None,
            scaling_grid_changes: Vec::new(),
            timeline: Timeline { commands: Vec::new(), is64 },
            dictionary_ids: HashMap::new(),
            dictionary_reference_ids: Vec::new(),
            texture_resource_name: String::new(),
            removed: BTreeSet::new(),
            object_changes: Vec::new(),
            texture_changes: Vec::new(),
            timeline_changes: Vec::new(),
            sprite_timelines: Vec::new(),
            dictionary_fixup_entries: Vec::new(),
            abc_blocks: Vec::new(),
            original_abc: Vec::new(),
            original_fields: fixups,
            symbol_class_names: Vec::new(),
            original_text_symbols: HashSet::new(),
        };
        swf.load(&mut stream)?;
        Ok(swf)
    }

    fn range(&self, pos: u64, size: u64) -> Result<()> {
        if pos.checked_add(size).is_none_or(|e| e > self.original.len() as u64) {
            return err(format!("Iggy reference outside main block: {pos}"));
        }
        Ok(())
    }

    fn pointer_at(&self, s: &mut Stream, pos: u64) -> Result<u64> {
        self.range(pos, self.ptr)?;
        s.seek(pos)?;
        let t = s.pointer(self.is64)?;
        if t != 0 {
            self.range(t, 1)?;
        }
        Ok(t)
    }

    fn wide_string(&self, s: &mut Stream, a: u64) -> Result<Option<String>> {
        if a == 0 {
            return Ok(None);
        }
        self.range(a, 2)?;
        s.seek(a)?;
        Ok(Some(s.wchar()?))
    }

    fn byte_string(&self, a: u64) -> Result<Option<String>> {
        if a == 0 {
            return Ok(None);
        }
        self.range(a, 1)?;
        let rest = &self.original[a as usize..];
        let Some(end) = rest.iter().position(|&b| b == 0) else { return err("Unterminated Iggy source name") };
        Ok(Some(String::from_utf8_lossy(&rest[..end]).into_owned()))
    }

    fn named_frames(&self, s: &mut Stream, a: u64, count: u64) -> Result<Vec<NamedFrame>> {
        let mut out = Vec::new();
        if count == 0 {
            return Ok(out);
        }
        if a == 0 {
            return err("Missing Iggy named-frame table");
        }
        let stride = if self.is64 { 16 } else { 8 };
        self.range(a, count * stride)?;
        for i in 0..count {
            let p = a + stride * i;
            let name = self.pointer_at(s, p)?;
            s.seek(p + self.ptr)?;
            let frame = s.u32()?;
            let reserved = if self.is64 { s.u32()? } else { 0 };
            let name = self.wide_string(s, name)?.unwrap_or_default();
            out.push(NamedFrame { name, frame, reserved });
        }
        Ok(out)
    }

    fn symbols(&self, s: &mut Stream, a: u64, count: u64) -> Result<Vec<SymbolClass>> {
        let mut out = Vec::new();
        if count == 0 {
            return Ok(out);
        }
        if a == 0 {
            return err("Missing Iggy symbol table");
        }
        let stride = if self.is64 { 24 } else { 12 };
        self.range(a, count * stride)?;
        for i in 0..count {
            let p = a + stride * i;
            let object = self.pointer_at(s, p)?;
            let name = self.pointer_at(s, p + self.ptr)?;
            s.seek(p + 2 * self.ptr)?;
            let id = s.u32()?;
            let reserved = if self.is64 { s.u32()? } else { 0 };
            if id != 0 && id >= self.dictionary.len() as u64 {
                return err("Invalid Iggy symbol dictionary ID");
            }
            let name = self.wide_string(s, name)?.unwrap_or_default();
            out.push(SymbolClass { object_address: object, name, character_id: id, reserved });
        }
        Ok(out)
    }

    fn load(&mut self, s: &mut Stream) -> Result<()> {
        self.source_name = self.byte_string(self.hdr.source_name_address())?;
        let name = self.wide_string(s, self.hdr.flash_filename_address())?;
        self.name = match name {
            Some(n) if !n.is_empty() => n,
            _ => match &self.source_name {
                Some(n) if !n.is_empty() => n.clone(),
                _ => "movie.swf".into(),
            },
        };
        let base = self.hdr.base_address();
        let count = self.hdr.dictionary_size();
        if count > 0 && base == 0 {
            return err("Missing Iggy dictionary");
        }
        self.range(base, count * self.ptr)?;
        let mut visited = HashSet::new();
        let h32 = self.hdr.is_h32();
        for id in 0..count as i32 {
            let address = self.pointer_at(s, base + self.ptr * id as u64)?;
            let ty = if address == 0 { -1 } else { self.original[address as usize] as i32 };
            self.dictionary.push(DictEntry { id, address, ty });
            if address != 0 {
                self.dictionary_ids.entry(address).or_insert(id);
            }
            if ty == 3 {
                self.range(address, if self.is64 { 72 } else { 36 })?;
                let t = self.pointer_at(s, address + if self.is64 { 56 } else { 28 })?;
                let tl = Timeline::read(&self.original, t, self.is64)?;
                put(&mut self.sprite_timelines, id, tl);
            }
            if address == 0 || !visited.insert(address) {
                continue;
            }
            s.seek(address)?;
            if ty == 22 {
                let mut font = if self.is64 { Font::read(s)? } else { Font::read32(s, h32)? };
                let snap = serialize(self.is64, &self.original, &self.original_fields, Some(address), Obj::Font(&mut font))?.data;
                self.fonts.push(font);
                self.font_addresses.push(address);
                self.font_snapshots.push(snap);
            }
            if ty == 6 {
                let text = if self.is64 { Text::read(s)? } else { Text::read32(s)? };
                if text.symbol_class_name.is_some() {
                    self.original_text_symbols.insert(text.text_index);
                }
                let snap = serialize(self.is64, &self.original, &self.original_fields, Some(address), Obj::Text(&text))?.data;
                self.texts.push(text);
                self.text_addresses.push(address);
                self.text_snapshots.push(snap);
            }
        }
        self.frame_labels = self.named_frames(s, self.hdr.frame_labels_address(), self.hdr.frame_label_count())?;
        self.timeline = Timeline::read(&self.original, self.hdr.timeline_address(), self.is64)?;
        self.scenes = self.named_frames(s, self.hdr.scenes_address(), self.hdr.scene_count())?;
        self.symbol_classes = self.symbols(s, self.hdr.symbol_classes_address(), self.hdr.symbol_class_count())?;
        let ic = self.hdr.import_count();
        if ic > 0 {
            let a = self.hdr.imports_address();
            if a == 0 {
                return err("Missing Iggy imports");
            }
            let stride = if self.is64 { 24 } else { 12 };
            self.range(a, ic * stride)?;
            for i in 0..ic {
                let p = a + stride * i;
                let lib = self.pointer_at(s, p)?;
                let sym = self.pointer_at(s, p + self.ptr)?;
                s.seek(p + 2 * self.ptr)?;
                let n = s.u32()?;
                let library = self.wide_string(s, lib)?.unwrap_or_default();
                let symbols = self.symbols(s, sym, n)?;
                self.imports.push(Import { library, symbols });
            }
        }
        let table = self.hdr.dictionary_fixups_address();
        if table != 0 {
            self.range(table, self.ptr + 4)?;
            let entries = self.pointer_at(s, table)?;
            s.seek(table + self.ptr)?;
            let n = s.u32()?;
            if n > 0 && entries == 0 {
                return err("Missing Iggy dictionary fixups");
            }
            self.range(entries, n * 8)?;
            for i in 0..n {
                s.seek(entries + 8 * i)?;
                let dest = s.u32()?;
                let id = s.u32()?;
                self.range(dest, self.ptr)?;
                self.dictionary_fixup_entries.push(entries + 8 * i);
                put(&mut self.dictionary_reference_ids, dest, id as i32);
                if id >= self.dictionary.len() as u64 {
                    return err("Invalid dictionary fixup ID");
                }
            }
        }
        let mut visited = HashSet::new();
        let mut a = self.hdr.abc_address();
        while a != 0 {
            self.range(a, self.ptr + 4)?;
            if !visited.insert(a) {
                return err("Cycle in Iggy ABC list");
            }
            s.seek(a)?;
            let abc = DeclStrings::read(s, self.is64)?;
            a = abc.next_address;
            self.original_abc.push(abc.clone());
            self.abc_blocks.push(abc);
        }
        Ok(())
    }

    pub fn set_default_name(&mut self, name: &str) {
        if self.source_name.as_deref().unwrap_or("").is_empty() && self.hdr.flash_filename_address() == 0 {
            self.name = name.into();
        }
    }

    pub fn original_data(&self) -> &[u8] {
        &self.original
    }

    pub fn frame_labels(&self) -> &[NamedFrame] {
        self.frame_label_changes.as_deref().unwrap_or(&self.frame_labels)
    }

    pub fn scenes(&self) -> &[NamedFrame] {
        self.scene_changes.as_deref().unwrap_or(&self.scenes)
    }

    pub fn symbol_classes(&self) -> Vec<SymbolClass> {
        self.symbol_changes
            .as_deref()
            .unwrap_or(&self.symbol_classes)
            .iter()
            .map(|s| {
                let mut s = s.clone();
                if let Some(n) = get(&self.symbol_class_names, &s.character_id) {
                    s.name = n.clone();
                }
                s
            })
            .collect()
    }

    pub fn original_symbol_classes(&self) -> &[SymbolClass] {
        &self.symbol_classes
    }

    pub fn imports(&self) -> &[Import] {
        self.import_changes.as_deref().unwrap_or(&self.imports)
    }

    pub fn original_imports(&self) -> &[Import] {
        &self.imports
    }

    fn legacy(&self) -> bool {
        !self.is64 && !self.hdr.is_h32()
    }

    pub fn set_imports(&mut self, libs: Vec<Import>) -> Result<()> {
        if libs.len() > 65535 {
            return err("Too many Iggy library imports");
        }
        let mut ids = HashSet::new();
        let original = import_ids(&self.imports);
        for l in &libs {
            import_name(&l.library)?;
            if l.symbols.len() > 65535 {
                return err("Too many Iggy imported symbols");
            }
            for s in &l.symbols {
                let id = s.character_id;
                import_name(&s.name)?;
                if id == 0 || id > 65535 || !ids.insert(id) {
                    return err(format!("Invalid or duplicate Iggy import character ID {id}"));
                }
                if (id as usize) < self.dictionary.len() && !self.dictionary[id as usize].is_null() && self.dictionary[id as usize].ty != 18 && !original.contains(&(id as i32)) {
                    return err(format!("Iggy import ID is occupied by a local object: {id}"));
                }
            }
        }
        let same = libs.len() == self.imports.len()
            && libs.iter().zip(&self.imports).all(|(a, b)| a.library == b.library && a.symbols.len() == b.symbols.len() && a.symbols.iter().zip(&b.symbols).all(|(x, y)| x.character_id == y.character_id && x.name == y.name));
        if !same && self.legacy() {
            return err("Library import table is unknown for the Iggy 32 legacy layout");
        }
        self.import_changes = if same { None } else { Some(libs) };
        Ok(())
    }

    pub fn dictionary_id(&self, reference_field: u64, address: u64) -> Result<i32> {
        if let Some(&id) = get(&self.dictionary_reference_ids, &reference_field) {
            return Ok(id);
        }
        if address == 0 {
            return Ok(-1);
        }
        match self.dictionary_ids.get(&address) {
            Some(&id) => Ok(id),
            None => err("Timeline object is outside the Iggy dictionary"),
        }
    }

    fn root_frame_count_offset(&self) -> u64 {
        if self.is64 {
            68
        } else if self.hdr.is_h32() {
            44
        } else {
            32
        }
    }

    pub fn timeline_frame_count(&self, id: i32) -> Result<u64> {
        let mut s = Stream::reader(&self.original);
        if id == 0 {
            s.seek(self.root_frame_count_offset())?;
            return s.u32();
        }
        if get(&self.sprite_timelines, &id).is_none() {
            return err("Unknown Iggy sprite ID");
        }
        s.seek(self.dictionary[id as usize].address + if self.is64 { 40 } else { 20 })?;
        Ok(s.u16()? as u64)
    }

    pub fn replace_timeline(&mut self, id: i32, r: Option<Replacement>) -> Result<()> {
        if id != 0 && !self.sprite_ids().contains(&id) {
            return err(format!("Missing Iggy sprite definition {id}"));
        }
        match r {
            None => self.timeline_changes.retain(|e| e.0 != id),
            Some(r) => put(&mut self.timeline_changes, id, r),
        }
        Ok(())
    }

    pub fn set_active_definitions(&mut self, ids: &HashSet<i32>) {
        self.removed.clear();
        let mut textures = Vec::new();
        for e in &self.dictionary {
            if e.id > 0 && matches!(e.ty, 1..=7 | 20..=22) && !ids.contains(&e.id) {
                self.removed.insert(e.id);
                if e.ty == 4 {
                    textures.push(e.id as u32);
                }
            }
        }
        for t in textures {
            self.replace_texture_record(t, None);
        }
        self.timeline_changes.retain(|e| e.0 == 0 || ids.contains(&e.0));
    }

    pub fn is_definition_removed(&self, id: i32) -> bool {
        self.removed.contains(&id)
    }

    pub fn sprite_ids(&self) -> Vec<i32> {
        let mut out: Vec<i32> = self.sprite_timelines.iter().map(|e| e.0).collect();
        for (id, o) in &self.object_changes {
            if o.ty == 3 && !out.contains(id) {
                out.push(*id);
            }
        }
        out.retain(|id| !self.removed.contains(id));
        out
    }

    /// `replaceDefinition`: a font or text written as a new object.
    pub fn replace_definition(&mut self, obj: Obj) -> Result<()> {
        let (id, size, ty) = match &obj {
            Obj::Font(f) => (f.font_id as i32, if self.is64 { 352 } else { f.x32.as_ref().map_or(208, |x| x.header_size as u32) }, 22),
            Obj::Text(t) => (t.text_index as i32, if self.is64 { 104 } else { 68 }, 6),
        };
        let ser = serialize(self.is64, &self.original, &self.original_fields, None, obj)?;
        let mut fields = ser.fields;
        if !fields.iter().any(|f| f.offset == 1) {
            fields.push(field(1, 12));
        }
        let cp = 2 * self.ptr;
        if !fields.iter().any(|f| f.offset == cp) {
            fields.push(field(cp, 2));
        }
        self.replace_object(id, ObjectData { data: ser.data, fields, characters: Vec::new(), address: 0, header_size: size, ty })
    }

    pub fn clear_asset_changes(&mut self) {
        self.object_changes.clear();
        self.texture_changes.clear();
    }

    pub fn replace_object(&mut self, id: i32, o: ObjectData) -> Result<()> {
        if id <= 0 || id > 65535 {
            return err("Invalid Iggy asset ID");
        }
        put(&mut self.object_changes, id, o);
        Ok(())
    }

    pub fn replace_texture_record(&mut self, id: u32, record: Option<Vec<u8>>) {
        put(&mut self.texture_changes, id, record);
    }

    pub fn texture_changes(&self) -> &[(u32, Option<Vec<u8>>)] {
        &self.texture_changes
    }

    pub fn set_symbol_class_name(&mut self, id: u64, name: String) -> Result<()> {
        if self.symbol_classes.iter().any(|s| s.character_id == id) {
            put(&mut self.symbol_class_names, id, name.clone());
            for t in &mut self.texts {
                if t.text_index as u64 == id && t.symbol_class_name.is_some() {
                    t.set_symbol_class_name(Some(name.clone()));
                }
            }
            return Ok(());
        }
        if let Some(t) = self.texts.iter_mut().find(|t| t.text_index as u64 == id) {
            t.set_symbol_class_name(Some(name));
            return Ok(());
        }
        err("Adding new Iggy SymbolClass bindings is not supported")
    }

    pub fn has_original_text_symbol_class(&self, id: u32) -> bool {
        self.original_text_symbols.contains(&id)
    }

    pub fn has_as3(&self) -> bool {
        self.as3_change.unwrap_or(self.hdr.has_as3())
    }

    pub fn set_as3(&mut self, enabled: bool) -> Result<()> {
        if enabled && self.legacy() {
            return err("AS3 root location is unknown for the Iggy 32 legacy layout");
        }
        self.as3_change = if enabled == self.hdr.has_as3() { None } else { Some(enabled) };
        Ok(())
    }

    pub fn set_symbol_classes(&mut self, symbols: Vec<SymbolClass>) -> Result<()> {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for s in &symbols {
            if s.character_id > 65535 || !ids.insert(s.character_id) || s.name.is_empty() || s.name.contains('\0') || !names.insert(s.name.clone()) {
                return err("Invalid or duplicate Iggy SymbolClass binding");
            }
        }
        let same = symbols.len() == self.symbol_classes.len() && symbols.iter().zip(&self.symbol_classes).all(|(a, b)| a.character_id == b.character_id && a.name == b.name);
        if !same && self.legacy() {
            return err("SymbolClass table is unknown for the Iggy 32 legacy layout");
        }
        self.symbol_changes = if same { None } else { Some(symbols) };
        self.symbol_class_names.clear();
        Ok(())
    }

    fn named_changes(before: &[NamedFrame], after: Vec<NamedFrame>) -> Result<Option<Vec<NamedFrame>>> {
        let mut same = before.len() == after.len();
        for (i, f) in after.iter().enumerate() {
            if f.name.contains('\0') || f.frame > 0x3fff_ffff {
                return err("Invalid Iggy scene or frame label");
            }
            if same {
                same = f.frame == before[i].frame && f.name == before[i].name;
            }
        }
        Ok(if same { None } else { Some(after) })
    }

    pub fn set_named_frames(&mut self, scenes: Vec<NamedFrame>, labels: Vec<NamedFrame>) -> Result<()> {
        self.scene_changes = Self::named_changes(&self.scenes, scenes)?;
        self.frame_label_changes = Self::named_changes(&self.frame_labels, labels)?;
        Ok(())
    }

    fn original_scaling_grid(&self, id: i32) -> Result<Option<[f32; 4]>> {
        if id <= 0 || id as usize >= self.dictionary.len() || self.dictionary[id as usize].ty != 3 {
            if get(&self.object_changes, &id).is_some_and(|o| o.ty == 3) {
                return Ok(None);
            }
            return err(format!("Iggy scaling grids require a sprite: {id}"));
        }
        let f = self.dictionary[id as usize].address + if self.is64 { 32 } else { 16 };
        self.range(f, self.ptr)?;
        let off = read_reference(&self.original, f, self.is64);
        if off == 0 || off == 1 {
            return Ok(None);
        }
        let mut t = f.wrapping_add(off);
        if !self.is64 {
            t &= 0xffff_ffff;
        }
        self.range(t, 16)?;
        let mut r = [0f32; 4];
        for (i, v) in r.iter_mut().enumerate() {
            let o = t as usize + 4 * i;
            *v = f32::from_le_bytes(self.original[o..o + 4].try_into().unwrap());
        }
        Ok(Some(r))
    }

    pub fn scaling_grid(&self, id: i32) -> Result<Option<[f32; 4]>> {
        match get(&self.scaling_grid_changes, &id) {
            Some(g) => Ok(*g),
            None => self.original_scaling_grid(id),
        }
    }

    pub fn set_scaling_grids(&mut self, grids: &[(i32, [f32; 4])]) -> Result<()> {
        for (id, _) in grids {
            self.original_scaling_grid(*id)?;
        }
        let mut changes = Vec::new();
        for id in self.sprite_ids() {
            let grid = get(grids, &id).copied();
            if let Some(g) = grid {
                if !g.iter().all(|v| v.is_finite()) || g[0] > g[2] || g[1] > g[3] {
                    return err("Invalid Iggy scaling grid");
                }
            }
            let original = self.original_scaling_grid(id)?;
            let mut same = grid.is_none() == original.is_none();
            if let (true, Some(g), Some(o)) = (same, grid, original) {
                same = (0..4).all(|i| java_round(g[i] * 20.0) == java_round(o[i] * 20.0));
            }
            if !same {
                changes.push((id, grid));
            }
        }
        self.scaling_grid_changes = changes;
        Ok(())
    }

    fn set_pointer(&self, d: &mut Stream, pos: u64, target: u64) {
        let saved = d.pos;
        d.pos = pos as usize;
        d.write_pointer(self.is64, target);
        d.pos = saved;
    }

    fn write_scalar(d: &mut Stream, fields: &mut Fields, pos: u64, v: u64, word: bool) {
        let saved = d.pos;
        d.pos = pos as usize;
        if word {
            d.w16(v as u32);
        } else {
            d.w32(v);
        }
        d.pos = saved;
        fields.put(field(pos, if word { 4 } else { 5 }));
    }

    fn write_string(d: &mut Stream, fields: &mut Fields, text: &str) -> u64 {
        let at = d.position();
        d.write_wchar(text);
        for i in 0..=wlen(text) {
            fields.put(field(at + 2 * i, 4));
        }
        at
    }

    fn write_timelines(&self, d: &mut Stream, fields: &mut Fields, refs: &mut Vec<(u64, u64)>, replaced: &mut HashSet<u64>, dict: &BTreeMap<i32, u64>) -> Result<Vec<(u64, i32)>> {
        let mut imports = Vec::new();
        let imported = import_ids(self.imports());
        let h32 = self.hdr.is_h32();
        let is64 = self.is64;
        for (id, r) in &self.timeline_changes {
            d.pad8();
            let base = d.position();
            d.write_bytes(&r.data);
            for f in &r.fields {
                fields.put(field(base + f.offset, f.op));
            }
            for &(off, cid) in &r.characters {
                let Some(&target) = dict.get(&cid).filter(|_| cid > 0) else { return err(format!("Invalid Iggy timeline character ID {cid}")) };
                let f = base + off;
                self.set_pointer(d, f, if imported.contains(&cid) { 0 } else { target });
                if target != 0 && !imported.contains(&cid) {
                    put(refs, f, target);
                } else {
                    put(&mut imports, f, cid);
                }
            }
            let first = if r.first < 0 { 0 } else { base + r.first as u64 };
            let last = if r.last < 0 { 0 } else { base + r.last as u64 };
            let (head, tail);
            if *id == 0 {
                head = if is64 { 16 } else { 8 };
                tail = if is64 { 24 } else { 12 };
                Self::write_scalar(d, fields, if is64 { 72 } else if h32 { 48 } else { 36 }, r.max_depth as u32 as u64, false);
                Self::write_scalar(d, fields, self.root_frame_count_offset(), r.frame_count as u32 as u64, false);
                let mut labels = 0;
                if !r.labels.is_empty() {
                    d.pad8();
                    labels = d.position();
                    let stride = if is64 { 16 } else { 8 };
                    d.write_bytes(&vec![0u8; r.labels.len() * stride as usize]);
                    for (i, l) in r.labels.iter().enumerate() {
                        let rec = labels + stride * i as u64;
                        let name = Self::write_string(d, fields, &l.name);
                        self.set_pointer(d, rec, name);
                        fields.put(field(rec, 2));
                        Self::write_scalar(d, fields, rec + self.ptr, l.frame, false);
                    }
                }
                let lf = if is64 { 32 } else if h32 { 16 } else { 44 };
                self.set_pointer(d, lf, labels);
                replaced.insert(lf);
                fields.put(field(lf, 2));
                Self::write_scalar(d, fields, if is64 { 76 } else if h32 { 52 } else { 48 }, r.labels.len() as u64, false);
            } else {
                if r.frame_count < 0 || r.frame_count > 65535 || r.max_depth > 65535 {
                    return err("Iggy sprite frame count or depth exceeds UI16");
                }
                let object = dict[id];
                head = object + if is64 { 56 } else { 28 };
                tail = object + if is64 { 64 } else { 32 };
                Self::write_scalar(d, fields, object + if is64 { 40 } else { 20 }, r.frame_count as u64, true);
                Self::write_scalar(d, fields, object + if is64 { 42 } else { 22 }, r.max_depth as u32 as u64, true);
                let cache = object + if is64 { 48 } else { 24 };
                self.set_pointer(d, cache, 0);
                replaced.insert(cache);
                fields.put(field(cache, 2));
            }
            self.set_pointer(d, head, first);
            self.set_pointer(d, tail, last);
            replaced.insert(head);
            replaced.insert(tail);
            fields.put(field(head, 2));
            fields.put(field(tail, 2));
        }
        Ok(imports)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_objects(&self, d: &mut Stream, fields: &mut Fields, refs: &mut Vec<(u64, u64)>, replaced: &mut HashSet<u64>, moves: &mut Vec<Move>, imports: &mut Vec<(u64, i32)>) -> Result<BTreeMap<i32, u64>> {
        let is64 = self.is64;
        let mut dict: BTreeMap<i32, u64> = self.dictionary.iter().map(|e| (e.id, e.address)).collect();
        let mut bases = HashMap::new();
        let imported = import_ids(self.imports());
        let mut import_dict_changed = !self.removed.is_empty();
        for &id in &self.removed {
            dict.insert(id, 0);
        }
        let mut dict_size = self.dictionary.len();
        let common = if is64 { 32 } else { 16 };
        if let Some(changes) = &self.import_changes {
            let active = import_ids(changes);
            for id in import_ids(&self.imports) {
                if !active.contains(&id) {
                    import_dict_changed |= dict.get(&id).copied().unwrap_or(0) != 0;
                    dict.insert(id, 0);
                }
            }
            for id in active {
                dict_size = dict_size.max(id as usize + 1);
                dict.entry(id).or_insert(0);
            }
        }
        for (id, r) in &self.object_changes {
            d.pad8();
            let base = d.position();
            d.write_bytes(&r.data);
            let target = base + r.address;
            bases.insert(*id, base);
            dict.insert(*id, target);
            dict_size = dict_size.max(*id as usize + 1);
            for f in &r.fields {
                fields.put(field(base + f.offset, f.op));
            }
            if (*id as usize) < self.dictionary.len() && self.dictionary[*id as usize].address != 0 {
                let old = self.dictionary[*id as usize];
                let old_size = match old.ty {
                    1 => if is64 { 80 } else { 48 },
                    2 => if is64 { 128 } else { 88 },
                    4 => if is64 { 88 } else { 56 },
                    5 => if is64 { 80 } else { 60 },
                    7 => if is64 { 88 } else { 60 },
                    20 => if is64 { 48 } else { 24 },
                    _ => if is64 { 64 } else { 40 },
                };
                moves.push(Move { old: old.address, new: target, len: old_size });
                let end = d.pos;
                d.seek(target + 1)?;
                d.write_bytes(&self.original[old.address as usize + 1..(old.address + common) as usize]);
                d.pos = end;
                for f in &self.original_fields {
                    if f.offset < old.address || f.offset >= old.address + common {
                        continue;
                    }
                    let p = target + f.offset - old.address;
                    fields.put(field(p, f.op));
                    if f.op == 2 {
                        let off = read_reference(&self.original, f.offset, is64);
                        self.set_pointer(d, p, if off == 1 { 0 } else { f.offset.wrapping_add(off) });
                        if off != 1 {
                            put(refs, p, f.offset.wrapping_add(off));
                        }
                    }
                }
            }
        }
        for (id, r) in &self.object_changes {
            let base = bases[id];
            for &(off, cid) in &r.characters {
                let target = dict.get(&cid).copied();
                if cid <= 0 || cid as usize >= dict_size || (target.is_none_or(|t| t == 0) && !imported.contains(&cid)) {
                    return err(format!("Invalid Iggy asset reference ID {cid}"));
                }
                let f = base + off;
                let t = if imported.contains(&cid) { 0 } else { target.unwrap_or(0) };
                self.set_pointer(d, f, t);
                if t != 0 {
                    put(refs, f, t);
                } else {
                    put(imports, f, cid);
                }
            }
        }
        if import_dict_changed || dict_size != self.dictionary.len() || self.object_changes.iter().any(|(id, _)| (*id as usize) < self.dictionary.len() && self.dictionary[*id as usize].address == 0) {
            d.pad8();
            let array = d.position();
            d.write_bytes(&vec![0u8; dict_size * self.ptr as usize]);
            for i in 0..dict_size as i32 {
                let f = array + i as u64 * self.ptr;
                let t = dict.get(&i).copied().unwrap_or(0);
                self.set_pointer(d, f, t);
                fields.put(field(f, 2));
                if t != 0 {
                    put(refs, f, t);
                }
                dict.entry(i).or_insert(0);
            }
            if self.hdr.base_address() != 0 {
                moves.push(Move { old: self.hdr.base_address(), new: array, len: self.dictionary.len() as u64 * self.ptr });
            }
            self.set_pointer(d, 0, array);
            replaced.insert(0);
            fields.put(field(0, 2));
            Self::write_scalar(d, fields, if is64 { 64 } else if self.hdr.is_h32() { 40 } else { 4 }, dict_size as u64, false);
        }
        Ok(dict)
    }

    fn write_imports(&self, d: &mut Stream, fields: &mut Fields, refs: &mut Vec<(u64, u64)>, replaced: &mut HashSet<u64>) -> Result<()> {
        let Some(changes) = &self.import_changes else { return Ok(()) };
        let is64 = self.is64;
        let stride = if is64 { 24 } else { 12 };
        let mut originals: Vec<(u64, &SymbolClass)> = Vec::new();
        for l in &self.imports {
            for s in &l.symbols {
                put(&mut originals, s.character_id, s);
            }
        }
        let mut table = 0;
        if !changes.is_empty() {
            d.pad8();
            table = d.position();
            d.write_bytes(&vec![0u8; changes.len() * stride as usize]);
        }
        for (i, l) in changes.iter().enumerate() {
            let rec = table + i as u64 * stride;
            let name = Self::write_string(d, fields, &l.library);
            self.set_pointer(d, rec, name);
            fields.put(field(rec, 2));
            let mut syms = 0;
            if !l.symbols.is_empty() {
                d.pad8();
                syms = d.position();
                d.write_bytes(&vec![0u8; l.symbols.len() * stride as usize]);
            }
            self.set_pointer(d, rec + self.ptr, syms);
            fields.put(field(rec + self.ptr, 2));
            Self::write_scalar(d, fields, rec + 2 * self.ptr, l.symbols.len() as u64, false);
            if is64 {
                if let Some(j) = self.imports.iter().position(|o| o.library == l.library) {
                    let old = (self.hdr.imports_address() + j as u64 * stride + 20) as usize;
                    let end = d.pos;
                    d.pos = (rec + 20) as usize;
                    d.write_bytes(&self.original[old..old + 4]);
                    d.pos = end;
                }
            }
            for (j, s) in l.symbols.iter().enumerate() {
                let original = get(&originals, &s.character_id).copied();
                let at = syms + j as u64 * stride;
                let object = original.map_or(0, |o| o.object_address);
                self.set_pointer(d, at, object);
                fields.put(field(at, 2));
                if object != 0 {
                    put(refs, at, object);
                }
                let sn = Self::write_string(d, fields, &s.name);
                self.set_pointer(d, at + self.ptr, sn);
                fields.put(field(at + self.ptr, 2));
                Self::write_scalar(d, fields, at + 2 * self.ptr, s.character_id, false);
                if is64 {
                    Self::write_scalar(d, fields, at + 20, original.map_or(0, |o| o.reserved), false);
                }
            }
        }
        let f = if is64 { 112 } else { 80 };
        self.set_pointer(d, f, table);
        replaced.insert(f);
        fields.put(field(f, 2));
        Self::write_scalar(d, fields, if is64 { 128 } else { 88 }, changes.len() as u64, false);
        Ok(())
    }

    fn write_named_table(&self, d: &mut Stream, fields: &mut Fields, replaced: &mut HashSet<u64>, values: &Option<Vec<NamedFrame>>, scene: bool) {
        let Some(values) = values else { return };
        let is64 = self.is64;
        let h32 = self.hdr.is_h32();
        let stride = if is64 { 16 } else { 8 };
        let mut table = 0;
        if !values.is_empty() {
            d.pad8();
            table = d.position();
            d.write_bytes(&vec![0u8; values.len() * stride as usize]);
        }
        let original = if scene { &self.scenes } else { &self.frame_labels };
        for (i, v) in values.iter().enumerate() {
            let at = table + stride * i as u64;
            let name = Self::write_string(d, fields, &v.name);
            self.set_pointer(d, at, name);
            fields.put(field(at, 2));
            Self::write_scalar(d, fields, at + self.ptr, v.frame, false);
            if is64 {
                Self::write_scalar(d, fields, at + 12, original.get(i).map_or(0, |o| o.reserved), false);
            }
        }
        let f = if is64 { if scene { 40 } else { 32 } } else if h32 { if scene { 20 } else { 16 } } else if scene { 56 } else { 44 };
        let count = if is64 { if scene { 80 } else { 76 } } else if h32 { if scene { 56 } else { 52 } } else if scene { 52 } else { 48 };
        self.set_pointer(d, f, table);
        replaced.insert(f);
        fields.put(field(f, 2));
        Self::write_scalar(d, fields, count, values.len() as u64, false);
    }

    fn write_movie_tags(&self, d: &mut Stream, fields: &mut Fields, refs: &mut Vec<(u64, u64)>, replaced: &mut HashSet<u64>, dict: &BTreeMap<i32, u64>, moves: &[Move]) -> Result<()> {
        let is64 = self.is64;
        if let Some(as3) = self.as3_change {
            let end = d.pos;
            let f = if is64 { 168 } else { 112 };
            d.pos = f as usize;
            d.w8(as3 as u32);
            d.pos = end;
            fields.put(field(f, if is64 { 6 } else { 5 }));
        }
        for (id, grid) in &self.scaling_grid_changes {
            let f = dict[id] + if is64 { 32 } else { 16 };
            let mut target = 0;
            if let Some(g) = grid {
                d.pad8();
                target = d.position();
                for &v in g {
                    let at = d.position();
                    d.wf32(v);
                    fields.put(field(at, 5));
                }
            }
            self.set_pointer(d, f, target);
            replaced.insert(f);
            fields.put(field(f, 2));
        }
        self.write_named_table(d, fields, replaced, &self.scene_changes, true);
        self.write_named_table(d, fields, replaced, &self.frame_label_changes, false);
        let Some(changes) = &self.symbol_changes else { return Ok(()) };
        let mi = move_index(moves);
        let stride = if is64 { 24 } else { 12 };
        let mut table = 0;
        if !changes.is_empty() {
            d.pad8();
            table = d.position();
            d.write_bytes(&vec![0u8; changes.len() * stride as usize]);
        }
        let imported = import_ids(self.imports());
        let mut class_names: Vec<(i32, u64)> = Vec::new();
        for s in &self.symbol_classes {
            put(&mut class_names, s.character_id as i32, 0);
        }
        for (i, s) in changes.iter().enumerate() {
            let id = s.character_id as i32;
            let object = dict.get(&id).copied();
            if id != 0 && object.is_none() {
                return err(format!("Invalid Iggy SymbolClass character ID {id}"));
            }
            let at = table + stride * i as u64;
            let before = self.symbol_classes.iter().find(|o| o.character_id == s.character_id);
            let target = if id == 0 || imported.contains(&id) || before.is_some_and(|b| b.object_address == 0) { 0 } else { object.unwrap_or(0) };
            self.set_pointer(d, at, target);
            fields.put(field(at, 2));
            if target != 0 {
                put(refs, at, target);
            }
            let name = Self::write_string(d, fields, &s.name);
            put(&mut class_names, id, name);
            self.set_pointer(d, at + self.ptr, name);
            fields.put(field(at + self.ptr, 2));
            Self::write_scalar(d, fields, at + 2 * self.ptr, id as u64, false);
            if is64 {
                Self::write_scalar(d, fields, at + 20, before.map_or(0, |b| b.reserved), false);
            }
        }
        let f = if is64 { 160 } else { 108 };
        self.set_pointer(d, f, table);
        replaced.insert(f);
        fields.put(field(f, 2));
        Self::write_scalar(d, fields, if is64 { 176 } else { 120 }, changes.len() as u64, false);
        for (id, name) in class_names {
            let object = dict.get(&id).copied().unwrap_or(0);
            if id == 0 || imported.contains(&id) || object == 0 {
                continue;
            }
            let nf = relocated(object, &mi) + if is64 { 16 } else { 8 };
            self.set_pointer(d, nf, name);
            replaced.insert(nf);
            refs.retain(|r| r.0 != nf);
            fields.put(field(nf, 2));
            let rf = nf + self.ptr;
            let end = d.pos;
            d.pos = rf as usize;
            if is64 {
                d.w64(0);
            } else {
                d.w32(0);
            }
            d.pos = end;
            replaced.insert(rf);
            refs.retain(|r| r.0 != rf);
            fields.remove(rf);
        }
        Ok(())
    }

    /// The edited main block and its relocation fields.
    pub fn write(&mut self) -> Result<(Vec<u8>, Vec<Field>)> {
        let is64 = self.is64;
        let mut hs = Stream::indexed();
        self.hdr.write(&mut hs);
        let header = hs.data;
        let mut d = Stream::new();
        d.write_bytes(&self.original);
        d.pos = 0;
        d.write_bytes(&header);
        d.pos = self.original.len();
        let mut fields = Fields::default();
        for f in &self.original_fields {
            fields.put(*f);
        }
        let mut moves = Vec::new();
        let mut inherited: Vec<(u64, u64)> = Vec::new();
        let mut replaced = HashSet::new();
        let mut changed = header[..] != self.original[..self.hdr.header_size()];
        if self.symbol_changes.is_none() {
            for (i, s) in self.symbol_classes.iter().enumerate() {
                let Some(n) = get(&self.symbol_class_names, &s.character_id) else { continue };
                if *n == s.name {
                    continue;
                }
                changed = true;
                d.pad8();
                let a = Self::write_string(&mut d, &mut fields, n);
                let np = self.hdr.symbol_classes_address() + if is64 { 24 } else { 12 } * i as u64 + self.ptr;
                self.set_pointer(&mut d, np, a);
                replaced.insert(np);
            }
        }
        let nf = self.fonts.len();
        for k in 0..nf + self.texts.len() {
            let (id, old_address, is_font) = if k < nf { (self.fonts[k].font_id as i32, self.font_addresses[k], true) } else { (self.texts[k - nf].text_index as i32, self.text_addresses[k - nf], false) };
            if self.removed.contains(&id) {
                continue;
            }
            let ser = if is_font {
                let mut f = std::mem::replace(&mut self.fonts[k], Font::create(true, false, 0)?);
                let r = serialize(is64, &self.original, &self.original_fields, Some(old_address), Obj::Font(&mut f));
                self.fonts[k] = f;
                r?
            } else {
                serialize(is64, &self.original, &self.original_fields, Some(old_address), Obj::Text(&self.texts[k - nf]))?
            };
            let snap = if is_font { &self.font_snapshots[k] } else { &self.text_snapshots[k - nf] };
            if ser.data == *snap {
                continue;
            }
            changed = true;
            d.pad8();
            let new_address = d.position();
            let body = if is_font { if is64 { 352 } else { self.fonts[k].x32.as_ref().unwrap().header_size as u64 } } else if is64 { 104 } else { 68 };
            moves.push(Move { old: old_address, new: new_address, len: body });
            d.write_bytes(&ser.data);
            if is_font {
                if let Some(x) = &self.fonts[k].x32 {
                    for &(old, new) in &x.relocated_parts {
                        moves.push(Move { old, new: new_address + new, len: x.part_sizes[&old] });
                    }
                    for &(p, t) in &x.external_references {
                        put(&mut inherited, new_address + p, t);
                    }
                }
            }
            for f in &ser.fields {
                let p = new_address + f.offset;
                fields.put(field(p, f.op));
                let generated = if !is_font {
                    self.texts[k - nf].is_generated_pointer(f.offset)
                } else if is64 {
                    matches!(f.offset, 48 | 56 | 64 | 96 | 128)
                } else {
                    matches!(f.offset, 28 | 32 | 36 | 60 | 76)
                };
                if f.op == 2 && !generated && f.offset < body && old_address + f.offset + self.ptr <= self.original.len() as u64 {
                    let op = old_address + f.offset;
                    let ov = read_reference(&self.original, op, is64);
                    if ov != 1 && read_reference(&ser.data, f.offset, is64) == ov {
                        put(&mut inherited, p, op.wrapping_add(ov));
                    }
                }
            }
        }
        let mut timeline_imports = Vec::new();
        let dict = self.write_objects(&mut d, &mut fields, &mut inherited, &mut replaced, &mut moves, &mut timeline_imports)?;
        self.write_imports(&mut d, &mut fields, &mut inherited, &mut replaced)?;
        for (k, v) in self.write_timelines(&mut d, &mut fields, &mut inherited, &mut replaced, &dict)? {
            put(&mut timeline_imports, k, v);
        }
        self.write_movie_tags(&mut d, &mut fields, &mut inherited, &mut replaced, &dict, &moves)?;
        changed |= !self.timeline_changes.is_empty()
            || !self.object_changes.is_empty()
            || !self.removed.is_empty()
            || self.import_changes.is_some()
            || !self.scaling_grid_changes.is_empty()
            || self.scene_changes.is_some()
            || self.frame_label_changes.is_some()
            || self.symbol_changes.is_some()
            || self.as3_change.is_some();
        let abc_changed = self.abc_blocks.len() != self.original_abc.len() || self.abc_blocks.iter().zip(&self.original_abc).any(|(a, b)| a.address != b.address || a.data != b.data || a.address == 0);
        if abc_changed {
            let po = self.hdr.abc_pointer_offset();
            if po < 0 {
                return err("ABC root location is unknown for this 32-bit Iggy layout");
            }
            changed = true;
            let mut first = 0;
            let mut previous: Option<u64> = None;
            for abc in &self.abc_blocks {
                abc.validate()?;
                d.pad8();
                let p = d.position();
                if first == 0 {
                    first = p;
                }
                if let Some(prev) = previous {
                    self.set_pointer(&mut d, prev, p);
                }
                if abc.address != 0 {
                    moves.push(Move { old: abc.address, new: p, len: self.ptr + 4 });
                }
                d.write_pointer(is64, 0);
                d.w32(abc.data.len() as u64);
                d.write_bytes(&abc.data);
                fields.put(field(p, 2));
                fields.put(field(p + self.ptr, 5));
                previous = Some(p);
            }
            self.set_pointer(&mut d, po as u64, first);
            replaced.insert(po as u64);
            fields.put(field(po as u64, 2));
        }
        if changed && self.original_fields.is_empty() {
            return err("Cannot edit Iggy data without its relocation program");
        }
        if changed {
            let mi = move_index(&moves);
            let imported = import_ids(self.imports());
            if !timeline_imports.is_empty() || (self.import_changes.is_some() && !self.dictionary_reference_ids.is_empty()) {
                if self.legacy() {
                    return err("This Iggy layout cannot add unresolved import references");
                }
                let mut entries: Vec<(u64, i32)> = Vec::new();
                for &(f, id) in &self.dictionary_reference_ids {
                    if imported.contains(&id) {
                        put(&mut entries, relocated(f, &mi), id);
                    }
                }
                for (k, v) in timeline_imports {
                    put(&mut entries, k, v);
                }
                d.pad8();
                let table = d.position();
                d.write_pointer(is64, if entries.is_empty() { 0 } else { table + self.ptr + 4 });
                d.w32(entries.len() as u64);
                fields.put(field(table, 2));
                fields.put(field(table + self.ptr, 5));
                for (k, v) in entries {
                    let at = d.position();
                    d.w32(k);
                    d.w32(v as u32 as u64);
                    fields.put(field(at, 5));
                    fields.put(field(at + 4, 5));
                }
                let f = if is64 { 136 } else { 96 };
                self.set_pointer(&mut d, f, table);
                replaced.insert(f);
                fields.put(field(f, 2));
            }
            for &e in &self.dictionary_fixup_entries {
                let dest = u32::from_le_bytes(self.original[e as usize..e as usize + 4].try_into().unwrap()) as u64;
                let nd = relocated(dest, &mi);
                if nd != dest {
                    let saved = d.pos;
                    d.pos = e as usize;
                    d.w32(nd);
                    d.pos = saved;
                }
            }
            for f in &self.original_fields {
                if f.op != 2 || replaced.contains(&f.offset) {
                    continue;
                }
                let v = read_reference(&self.original, f.offset, is64);
                if v == 1 {
                    continue;
                }
                let old = f.offset.wrapping_add(v);
                let t = relocated(old, &mi);
                if t != old {
                    self.set_pointer(&mut d, f.offset, t);
                }
            }
            for &(p, t) in &inherited {
                self.set_pointer(&mut d, p, relocated(t, &mi));
            }
        }
        Ok((d.data, fields.values()))
    }

    /// The block with its relocation program.
    pub fn write_indexed(&mut self) -> Result<(Vec<u8>, Vec<u8>)> {
        let (data, fields) = self.write()?;
        let mut b = super::stream::IndexBuilder::new();
        write_fields(&mut b, &fields, data.len() as u64)?;
        Ok((data, b.bytes()))
    }
}

/// Java's `Math.round(float)`.
pub fn java_round(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        (v as f64 + 0.5).floor() as i32
    }
}
