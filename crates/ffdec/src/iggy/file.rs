//! `IggyFile`: the container header, its block table, relocation programs and texture records.

use std::collections::HashSet;

use super::model::{Font, Header, TextureMetadata};
use super::stream::{compact, err, field, write_fields, Field, Fixup, IndexBuilder, Stream};
use super::swf::{has32_root, Swf};
use crate::error::{Error, Result};

pub const MAGIC: u64 = 0xED0A_6749;
pub const HEADER_SIZE: u64 = 32;
pub const ENTRY_SIZE: u64 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    X86Legacy,
    X86,
    X64,
}

impl Variant {
    pub fn parse(v: &str) -> Result<Variant> {
        match v.trim().to_ascii_lowercase().as_str() {
            "32_legacy" => Ok(Variant::X86Legacy),
            "32" => Ok(Variant::X86),
            "64" => Ok(Variant::X64),
            _ => err(format!("Unknown Iggy variant: {v}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IggyHeader {
    pub version: u64,
    pub platform: [u32; 4],
    pub swf_version: u64,
    pub reserved: Vec<u8>,
    pub num_subfiles: u64,
    legacy_table: bool,
}

impl IggyHeader {
    fn read(s: &mut Stream, legacy_table: bool) -> Result<IggyHeader> {
        if s.u32()? != MAGIC {
            return err("Invalid Iggy file");
        }
        let version = s.u32()?;
        if version != 0x900 {
            return err(format!("Unsupported Iggy version: {version}"));
        }
        let platform = [s.u8()?, s.u8()?, s.u8()?, s.u8()?];
        if platform[1] != 32 && platform[1] != 64 {
            return err("Invalid Iggy pointer width");
        }
        if !matches!(platform[0], 1 | 2) || !matches!(platform[2], 1 | 2) {
            return err("Invalid Iggy data platform");
        }
        let mut h = IggyHeader { version, platform, swf_version: 0, reserved: Vec::new(), num_subfiles: 0, legacy_table };
        if h.is64() {
            h.require_platform_fixups()?;
        }
        h.swf_version = s.u32()?;
        h.reserved = s.bytes(12)?;
        h.num_subfiles = s.u32()?;
        if h.swapped_table() {
            h.num_subfiles = (h.num_subfiles as u32).swap_bytes() as u64;
        }
        Ok(h)
    }

    fn swapped_table(&self) -> bool {
        self.legacy_table && !self.is64() && self.platform[0] == 2
    }

    fn write(&self, s: &mut Stream) {
        s.w32(MAGIC);
        s.w32(self.version);
        for &p in &self.platform {
            s.w8(p);
        }
        s.w32(self.swf_version);
        s.write_bytes(&self.reserved);
        s.w32(if self.swapped_table() { (self.num_subfiles as u32).swap_bytes() as u64 } else { self.num_subfiles });
    }

    fn require_platform_fixups(&self) -> Result<()> {
        if (self.platform[0] != 1 && self.platform[3] & 1 == 0) || (self.platform[2] != 1 && self.platform[3] & 2 == 0) {
            return err("Iggy platform has no required fixup data");
        }
        Ok(())
    }

    pub fn is64(&self) -> bool {
        self.platform[1] == 64
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub ty: u32,
    pub copy_flag: u32,
    pub target_index: u32,
    pub metadata: u32,
    pub size: u64,
    pub size2: u64,
    pub offset: u64,
}

impl Entry {
    fn new(descriptor: u64, size: u64, size2: u64, offset: u64) -> Entry {
        Entry { ty: (descriptor & 255) as u32, copy_flag: ((descriptor >> 8) & 255) as u32, target_index: ((descriptor >> 16) & 255) as u32, metadata: ((descriptor >> 24) & 255) as u32, size, size2, offset }
    }

    fn read(s: &mut Stream) -> Result<Entry> {
        Ok(Entry { ty: s.u8()?, copy_flag: s.u8()?, target_index: s.u8()?, metadata: s.u8()?, size: s.u32()?, size2: s.u32()?, offset: s.u32()? })
    }

    fn write(&self, s: &mut Stream) {
        s.w8(self.ty);
        s.w8(self.copy_flag);
        s.w8(self.target_index);
        s.w8(self.metadata);
        s.w32(self.size);
        s.w32(self.size2);
        s.w32(self.offset);
    }
}

pub struct IggyFile {
    pub header: IggyHeader,
    pub entries: Vec<Entry>,
    pub data: Vec<Vec<u8>>,
    /// The block index each relocation entry targets, parallel to `entries`.
    targets: Vec<Option<usize>>,
    main: usize,
    legacy32: bool,
    pub swf: Swf,
    pub textures: Vec<TextureMetadata>,
    /// The file name it was read from, for the companion texture file.
    pub file_name: Option<String>,
}

impl IggyFile {
    /// Reads a container; `name` is the file name, for the default movie and texture names.
    pub fn read(bytes: &[u8], name: Option<&str>) -> Result<IggyFile> {
        match Self::read_container(bytes, false, name) {
            Ok(f) => Ok(f),
            Err(modern) => {
                let mut s = Stream::reader(bytes);
                let Ok(h) = IggyHeader::read(&mut s, false) else { return Err(modern.0) };
                if h.is64() {
                    return Err(modern.0);
                }
                match Self::read_container(bytes, true, name) {
                    Ok(f) => Ok(f),
                    Err(legacy) => Err(if modern.1 { legacy.0 } else { modern.0 }),
                }
            }
        }
    }

    fn read_container(bytes: &[u8], legacy: bool, name: Option<&str>) -> std::result::Result<IggyFile, (Error, bool)> {
        let plain = |e: Error| (e, false);
        let mut s = Stream::reader(bytes);
        let header = IggyHeader::read(&mut s, legacy).map_err(plain)?;
        let legacy32 = legacy && !header.is64();
        let table = header.num_subfiles * ENTRY_SIZE;
        if header.num_subfiles > i32::MAX as u64 || table > s.available() as u64 {
            return Err(plain(Error::Other("Invalid Iggy descriptor table size".into())));
        }
        let mut entries = Vec::new();
        for _ in 0..header.num_subfiles {
            let mut e = Entry::read(&mut s).map_err(plain)?;
            if legacy32 && header.platform[0] == 2 {
                e.size = (e.size as u32).swap_bytes() as u64;
                e.offset = (e.offset as u32).swap_bytes() as u64;
            }
            entries.push(e);
        }
        let mut data = Vec::new();
        for e in &entries {
            if e.size > i32::MAX as u64 || e.size > bytes.len() as u64 || e.offset > bytes.len() as u64 - e.size || e.offset < HEADER_SIZE + table {
                return Err(plain(Error::Other("Invalid Iggy block range".into())));
            }
            data.push(bytes[e.offset as usize..(e.offset + e.size) as usize].to_vec());
        }
        Self::parse_entries(header, entries, data, legacy32, name)
    }

    fn selects(e: &Entry, legacy32: bool) -> bool {
        e.ty == 1 || e.ty == 8 || (legacy32 && e.ty != 0)
    }

    fn parse_entries(mut header: IggyHeader, entries: Vec<Entry>, mut data: Vec<Vec<u8>>, legacy32: bool, name: Option<&str>) -> std::result::Result<IggyFile, (Error, bool)> {
        let plain = |e: Error| (e, false);
        let mut selected: Vec<usize> = Vec::new();
        let mut fields: Vec<Vec<Field>> = vec![Vec::new(); entries.len()];
        let mut normalized: Vec<HashSet<u64>> = vec![HashSet::new(); entries.len()];
        let mut targets = vec![None; entries.len()];
        let mut main = None;
        let le = header.platform[0] == 1;
        let native = header.platform[2] == 1;
        for (i, e) in entries.iter().enumerate() {
            if Self::selects(e, legacy32) {
                selected.push(i);
                if e.ty == 1 && main.is_none() {
                    main = Some(i);
                }
            } else if e.ty == 0 {
                let Some(&t) = selected.get(e.target_index as usize) else { return Err(plain(Error::Other("Invalid Iggy fixup target".into()))) };
                let program = Fixup::parse(header.is64(), &data[i], data[t].len() as u64).map_err(plain)?;
                if !le || !native {
                    for f in &program.fields {
                        if !normalized[t].insert(f.offset) {
                            return Err(plain(Error::Other("Overlapping Iggy platform fixups".into())));
                        }
                    }
                    program.normalize(&mut data[t], le, native).map_err(plain)?;
                }
                fields[t].extend(program.fields);
                targets[i] = Some(t);
            }
        }
        let Some(main) = main else { return Err(plain(Error::Other("Missing Iggy main block".into()))) };
        if !header.is64() && has32_root(&data[main], &fields[main]).map_err(plain)? == legacy32 {
            return Err((Error::Other("Iggy main layout does not match the container variant".into()), true));
        }
        if !legacy32 {
            header.require_platform_fixups().map_err(plain)?;
        }
        if !le || !native {
            for &e in &selected {
                if header.is64() && fields[e].is_empty() {
                    return Err(plain(Error::Other("Missing Iggy platform fixups".into())));
                }
            }
            header.platform[0] = 1;
            header.platform[2] = 1;
        }
        let mut textures = Vec::new();
        for &e in &selected {
            if entries[e].ty == 8 && !legacy32 {
                textures.push(TextureMetadata::parse(&data[e], header.is64()).map_err(plain)?);
            }
        }
        let mut swf = Swf::read(data[main].clone(), std::mem::take(&mut fields[main]), header.is64()).map_err(plain)?;
        if let Some(n) = name {
            let stem = n.rfind('.').map_or(n, |i| &n[..i]);
            swf.set_default_name(&format!("{stem}.swf"));
            swf.texture_resource_name = format!("{stem}.raw.iggytex");
        }
        Ok(IggyFile { header, entries, data, targets, main, legacy32, swf, textures, file_name: name.map(str::to_string) })
    }

    /// `IggyFile.create`: an empty movie container of the given layout.
    pub fn create(variant: Variant, swf_name: &str) -> Result<IggyFile> {
        if swf_name.is_empty() || swf_name.contains('\0') {
            return err("Missing Iggy variant or valid movie name");
        }
        let wide = variant == Variant::X64;
        let standard32 = variant == Variant::X86;
        let size = if wide { 184 } else if standard32 { 128 } else { 64 };
        let mut root = Stream::new();
        root.write_bytes(&vec![0u8; size]);
        let nulls: &[u64] = if wide { &[0, 8, 16, 24, 32, 40, 112, 120, 136, 144, 152, 160] } else if standard32 { &[0, 4, 8, 12, 16, 20, 80, 84, 96, 100, 104, 108] } else { &[0, 8, 12, 44, 56, 60] };
        for &o in nulls {
            root.seek(o)?;
            root.write_pointer(wide, 0);
        }
        root.seek(size as u64)?;
        let source = root.position();
        root.write_bytes(swf_name.as_bytes());
        root.w8(0);
        root.pad8();
        let name = root.position();
        if variant != Variant::X86Legacy {
            root.write_wchar(swf_name);
            root.pad8();
            root.seek(if wide { 144 } else { 100 })?;
            root.write_pointer(wide, name);
        }
        root.seek(if wide { 8 } else if standard32 { 4 } else { 60 })?;
        root.write_pointer(wide, source);
        let mut r = Stream::reader(&root.data);
        let mut hdr = if wide {
            Header::read64(&mut r)?
        } else if standard32 {
            Header::read32(&mut r)?
        } else {
            Header::read_legacy(&mut r)?
        };
        hdr.set_movie_properties((0, 0, 0, 0), 24.0, 0, 0xffff_ffff)?;
        let mut main = Stream::indexed();
        main.write_bytes(&root.data);
        main.pos = 0;
        hdr.write(&mut main);
        let program = main.idx().bytes();
        let data = main.data;
        let mut fields = Fixup::parse(wide, &program, data.len() as u64)?.fields;
        if variant != Variant::X86Legacy {
            for i in 0..=swf_name.encode_utf16().count() as u64 {
                fields.push(field(name + 2 * i, 4));
            }
        }
        let mut index = IndexBuilder::new();
        write_fields(&mut index, &fields, data.len() as u64)?;
        let programs = compact(wide, &index.bytes(), data.len() as u64)?;
        if programs.len() >= 256 {
            return err("Too many Iggy root relocation blocks");
        }
        let mut c = Stream::new();
        let n = programs.len() as u64 + 1;
        let mut offset = HEADER_SIZE + ENTRY_SIZE * n;
        IggyHeader { version: 0x900, platform: [1, if wide { 64 } else { 32 }, 1, 3], swf_version: 0, reserved: vec![0; 12], num_subfiles: n, legacy_table: true }.write(&mut c);
        Entry::new(1, data.len() as u64, 0, offset).write(&mut c);
        offset += data.len() as u64;
        for p in &programs {
            Entry::new(0, p.len() as u64, 0, offset).write(&mut c);
            offset += p.len() as u64;
        }
        c.write_bytes(&data);
        for p in &programs {
            c.write_bytes(p);
        }
        let mut f = IggyFile::read(&c.data, None)?;
        let stem = swf_name.rfind('.').map_or(swf_name, |i| &swf_name[..i]);
        f.swf.texture_resource_name = format!("{stem}.raw.iggytex");
        Ok(f)
    }

    pub fn swf_version(&self) -> u8 {
        let v = self.header.swf_version;
        if (3..=255).contains(&v) {
            v as u8
        } else {
            10
        }
    }

    pub fn variant(&self) -> Variant {
        if self.header.is64() {
            Variant::X64
        } else if self.swf.hdr.is_h32() {
            Variant::X86
        } else {
            Variant::X86Legacy
        }
    }

    pub fn is_legacy32(&self) -> bool {
        self.legacy32
    }

    /// `updateFlashEntry`: rewrites the main block, its texture records and relocations.
    pub fn update_flash_entry(&mut self) -> Result<()> {
        let (replacement, index) = self.swf.write_indexed()?;
        self.update_texture_entries()?;
        if replacement == self.data[self.main] {
            return Ok(());
        }
        let len = replacement.len() as u64;
        self.data[self.main] = replacement;
        self.entries[self.main].size = len;
        let programs = compact(self.header.is64(), &index, len)?;
        self.replace_fixup_programs(self.main, programs)?;
        Ok(())
    }

    fn replace_fixup_programs(&mut self, target: usize, programs: Vec<Vec<u8>>) -> Result<usize> {
        let previous = self.targets.iter().filter(|t| **t == Some(target)).count();
        if self.entries.len() - previous + programs.len() > 256 {
            return err("Too many Iggy blocks after partitioning relocation programs");
        }
        let proto_index = self.targets.iter().position(|t| *t == Some(target));
        let mut insertion = proto_index.unwrap_or(self.entries.len());
        let proto = match proto_index {
            Some(i) => self.entries[i].clone(),
            None => {
                let mut p = Entry::new(0, 0, 0, 0);
                for (i, e) in self.entries.iter().enumerate() {
                    if i == target {
                        break;
                    }
                    if Self::selects(e, self.legacy32) {
                        p.target_index += 1;
                    }
                }
                p
            }
        };
        let mut target = target;
        for i in (0..self.entries.len()).rev() {
            if self.targets[i] == Some(target) {
                self.entries.remove(i);
                self.data.remove(i);
                self.targets.remove(i);
                if i < target {
                    target -= 1;
                }
                if i < self.main {
                    self.main -= 1;
                }
                for t in self.targets.iter_mut().flatten() {
                    if *t > i {
                        *t -= 1;
                    }
                }
            }
        }
        for p in programs {
            let d = (proto.copy_flag as u64) << 8 | (proto.target_index as u64) << 16 | (proto.metadata as u64) << 24;
            let e = Entry::new(d, p.len() as u64, proto.size2, 0);
            self.entries.insert(insertion, e);
            self.data.insert(insertion, p);
            for t in self.targets.iter_mut().flatten() {
                if *t >= insertion {
                    *t += 1;
                }
            }
            if insertion <= target {
                target += 1;
            }
            if insertion <= self.main {
                self.main += 1;
            }
            self.targets.insert(insertion, Some(target));
            insertion += 1;
        }
        Ok(target)
    }

    fn update_texture_entries(&mut self) -> Result<()> {
        let changes = self.swf.texture_changes().to_vec();
        if changes.is_empty() {
            return Ok(());
        }
        let is64 = self.header.is64();
        let mut found = false;
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].ty != 8 || self.legacy32 {
                i += 1;
                continue;
            }
            found = true;
            let old = self.data[i].clone();
            let replacement = TextureMetadata::update(&old, is64, &changes)?;
            if old != replacement {
                let len = replacement.len() as u64;
                self.data[i] = replacement.clone();
                self.entries[i].size = len;
                i = self.update_texture_fixups(i, &old, &replacement)?;
            }
            i += 1;
        }
        if !found && changes.iter().any(|c| c.1.is_some()) {
            if self.legacy32 {
                return err("This Iggy 32 legacy layout does not support platform texture records");
            }
            let replacement = TextureMetadata::update(&[], is64, &changes)?;
            self.entries.push(Entry::new(8, replacement.len() as u64, 0, 0));
            self.data.push(replacement);
            self.targets.push(None);
        }
        Ok(())
    }

    fn update_texture_fixups(&mut self, target: usize, old: &[u8], replacement: &[u8]) -> Result<usize> {
        let is64 = self.header.is64();
        let hs = if is64 { 16 } else { 12 };
        let old_count = if old.is_empty() { 0 } else { old[0] as u64 | (old[1] as u64) << 8 };
        let count = replacement[0] as u64 | (replacement[1] as u64) << 8;
        let mut original = Vec::new();
        let mut found = false;
        for i in 0..self.entries.len() {
            if self.targets[i] != Some(target) {
                continue;
            }
            found = true;
            original.extend(Fixup::parse(is64, &self.data[i], old.len() as u64)?.fields);
        }
        if !found {
            return Ok(target);
        }
        let mut fields = Vec::new();
        let mut pattern: Vec<(u64, u32)> = Vec::new();
        for f in original {
            if f.offset < hs {
                fields.push(f);
            } else if f.offset < hs + 12 * old_count {
                let o = (f.offset - hs) % 12;
                match pattern.iter_mut().find(|p| p.0 == o) {
                    Some(p) if p.1 != f.op => return err("Inconsistent Iggy texture fixup layout"),
                    Some(_) => {}
                    None => pattern.push((o, f.op)),
                }
            } else {
                fields.push(field((f.offset as i64 + 12 * (count as i64 - old_count as i64)) as u64, f.op));
            }
        }
        for r in 0..count {
            for &(o, op) in &pattern {
                fields.push(field(hs + 12 * r + o, op));
            }
        }
        let mut b = IndexBuilder::new();
        write_fields(&mut b, &fields, replacement.len() as u64)?;
        let programs = compact(is64, &b.bytes(), replacement.len() as u64)?;
        self.replace_fixup_programs(target, programs)
    }

    /// The container bytes, after `update_flash_entry`.
    pub fn bytes(&mut self) -> Vec<u8> {
        self.header.num_subfiles = self.entries.len() as u64;
        let mut s = Stream::new();
        self.header.write(&mut s);
        let mut offset = HEADER_SIZE + ENTRY_SIZE * self.entries.len() as u64;
        for e in &mut self.entries {
            e.offset = offset;
            offset += e.size;
            e.write(&mut s);
        }
        for d in &self.data {
            s.write_bytes(d);
        }
        s.data
    }

    /// Saves edits: the updated main block and container.
    pub fn save(&mut self) -> Result<Vec<u8>> {
        self.update_flash_entry()?;
        Ok(self.bytes())
    }

    pub fn fonts(&self) -> &[Font] {
        &self.swf.fonts
    }
}
