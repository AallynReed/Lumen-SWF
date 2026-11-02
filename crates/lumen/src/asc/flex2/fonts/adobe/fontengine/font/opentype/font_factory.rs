//! Port of afe.jar `com.adobe.fontengine.font.opentype.FontFactory`: the offset-ordered list of blocks (TTC header,
//! font headers, tables) that streams an OpenType file or collection, copying overlapping tables from earlier blocks.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_input_stream::FontInputStream;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::open_type_font::OpenTypeFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::table::{AnyTable, Table};
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::tag::Tag;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{ExceptionKind, FontEngineException, FontResult};

enum Kind {
    /// `TableToLoad`.
    Table { tag: i32, table: Option<AnyTable> },
    /// `FontToLoad`.
    Font { tables: Vec<usize>, header: Option<OTByteArray> },
    /// `TTCToLoad`.
    Ttc { header: Option<OTByteArray> },
}

struct Block {
    next: Option<usize>,
    offset: i64,
    length: i32,
    kind: Kind,
}

struct Blocks {
    b: Vec<Block>,
}

fn npe() -> FontEngineException {
    FontEngineException::new(ExceptionKind::FontEngine, "java.lang.NullPointerException")
}

impl Blocks {
    fn push(&mut self, offset: i64, length: i32, kind: Kind) -> usize {
        self.b.push(Block { next: None, offset, length, kind });
        self.b.len() - 1
    }

    /// `TableToLoad.equals` (a font block is only equal to itself).
    fn equals(&self, a: usize, b: usize) -> bool {
        if a == b {
            return true;
        }
        match (&self.b[a].kind, &self.b[b].kind) {
            (Kind::Table { tag: t1, .. }, Kind::Table { tag: t2, .. }) => self.b[a].length == self.b[b].length && t1 == t2 && self.b[a].offset == self.b[b].offset,
            _ => false,
        }
    }

    /// `BlockToLoad.add(block)` starting at `this`: returns the block now in the list.
    fn add(&mut self, mut this: usize, nb: usize) -> usize {
        loop {
            let Some(next) = self.b[this].next else {
                self.b[this].next = Some(nb);
                return nb;
            };
            let (o, l) = (self.b[nb].offset, self.b[nb].length);
            if o < self.b[next].offset || o == self.b[next].offset && l < self.b[next].length {
                self.b[nb].next = Some(next);
                self.b[this].next = Some(nb);
                return nb;
            }
            if self.equals(nb, next) {
                return next;
            }
            this = next;
        }
    }

    /// `getByteArray()`: `Err` for Java's NullPointerException on a table that did not load.
    fn byte_array(&self, i: usize) -> FontResult<Option<&OTByteArray>> {
        Ok(match &self.b[i].kind {
            Kind::Table { table, .. } => table.as_ref().ok_or_else(npe)?.data(),
            Kind::Font { header, .. } => header.as_ref(),
            Kind::Ttc { .. } => None,
        })
    }

    fn load_table(&mut self, i: usize, head: usize, fis: &mut FontInputStream) -> FontResult<()> {
        let (offset, length) = (self.b[i].offset, self.b[i].length);
        let tag = match &self.b[i].kind {
            Kind::Table { tag, .. } => *tag,
            _ => unreachable!(),
        };
        let mut fba = None;
        if fis.get_current_offset() <= offset {
            fis.skip_to(offset)?;
            fba = Some(fis.read_font_byte_array(length)?);
        } else {
            let mut builder = OTByteArray::get_ot_byte_array_builder_instance_sized(length);
            let mut n = (fis.get_current_offset() - offset) as i32;
            if length < n {
                n = length;
            }
            let mut n2: i32 = 0;
            let mut b2 = Some(head);
            while let Some(b) = b2 {
                if self.b[b].offset + self.b[b].length as i64 >= offset {
                    break;
                }
                if self.b[b].next == Some(i) {
                    if Table::known_table(tag) {
                        return Err(FontEngineException::invalid_font("Cannot find overlapped data"));
                    }
                    b2 = None;
                    break;
                }
                b2 = self.b[b].next;
            }
            if b2.is_some() {
                while n2 < n {
                    let Some(b) = b2 else { break };
                    if self.b[b].offset > n2 as i64 + offset {
                        break;
                    }
                    let blen = self.b[b].length;
                    let boff = self.b[b].offset;
                    let a = self.byte_array(b)?.ok_or_else(|| FontEngineException::invalid_font("Overlapping data cannot be accessed."))?.clone();
                    let n3 = (n2 as i64 + offset - boff) as i32;
                    let n4 = n.min(blen - n3);
                    if n4 > 0 && (n3 < 0 || n3 as i64 + n4 as i64 > a.base.size as i64) {
                        return Err(FontEngineException::new(ExceptionKind::FontEngine, "java.lang.ArrayIndexOutOfBoundsException: Source buffer is too small"));
                    }
                    builder.replace_from(n2, &a.base, n3, n4)?;
                    b2 = self.b[b].next;
                    n2 += n4;
                }
                if n2 < n {
                    if Table::known_table(tag) {
                        return Err(FontEngineException::invalid_font("Data could not be copied"));
                    }
                } else {
                    if length > n2 {
                        fis.add_bytes(&mut builder.base.byte_array, length - n2, n2)?;
                    }
                    fba = Some(builder.to_ot_byte_array().base);
                }
            }
        }
        if let Some(a) = fba {
            let t = Table::factory(tag, a)?;
            if let Kind::Table { table, .. } = &mut self.b[i].kind {
                *table = Some(t);
            }
        }
        Ok(())
    }

    fn load_font(&mut self, i: usize, fis: &mut FontInputStream) -> FontResult<()> {
        let offset = self.b[i].offset;
        let has_header = matches!(&self.b[i].kind, Kind::Font { header: Some(_), .. });
        if !has_header {
            fis.skip_to(offset)?;
            let h = OTByteArray::from_font_byte_array(fis.read_font_byte_array(12)?);
            if let Kind::Font { header, .. } = &mut self.b[i].kind {
                *header = Some(h);
            }
        } else {
            fis.skip_to(offset + 12)?;
        }
        let header = match &self.b[i].kind {
            Kind::Font { header: Some(h), .. } => h.clone(),
            _ => unreachable!(),
        };
        let n = header.getint32(0)?;
        if Tag::misc_OTTO != n && Tag::misc_1_0 != n && Tag::misc_true != n {
            return Err(FontEngineException::invalid_font(format!("FontData header does not have correct version (is: 0x{:x})", n)));
        }
        let n2 = header.getuint16(4)?;
        let dir = OTByteArray::from_font_byte_array(fis.read_font_byte_array(16 * n2)?);
        for k in 0..n2 {
            let n3 = dir.getuint32asint(16 * k + 12, "OT tables larger than 2^31 bytes not supported")?;
            let n4 = dir.getint32(k * 16)?;
            if n3 == 0 {
                continue;
            }
            let off = dir.getuint32(16 * k + 8)?;
            let t = self.push(off, n3, Kind::Table { tag: n4, table: None });
            let t = self.add(i, t);
            if let Kind::Font { tables, .. } = &mut self.b[i].kind {
                tables.push(t);
            }
        }
        Ok(())
    }

    fn load_ttc(&mut self, i: usize, fis: &mut FontInputStream) -> FontResult<()> {
        let offset = self.b[i].offset;
        let has_header = matches!(&self.b[i].kind, Kind::Ttc { header: Some(_) });
        if !has_header {
            fis.skip_to(offset)?;
            let h = OTByteArray::from_font_byte_array(fis.read_font_byte_array(12)?);
            self.b[i].kind = Kind::Ttc { header: Some(h) };
        } else {
            fis.skip_to(offset + 12)?;
        }
        let header = match &self.b[i].kind {
            Kind::Ttc { header: Some(h) } => h.clone(),
            _ => unreachable!(),
        };
        if Tag::misc_ttcf as i64 != header.getuint32(0)? {
            return Err(FontEngineException::invalid_font("TTC header does not start with 'ttcf'"));
        }
        let n = header.getuint16(4)?;
        if n != 1 && n != 2 {
            return Err(FontEngineException::unsupported_font(format!("TTC major version {} is not supported", n)));
        }
        let n2 = header.getuint32asint(8, "TTC fonts with 2^31 fonts or more not supported")?;
        let a = OTByteArray::from_font_byte_array(fis.read_font_byte_array(4 * n2)?);
        for k in 0..n2 {
            let f = self.push(a.getuint32(4 * k)?, 12, Kind::Font { tables: Vec::new(), header: None });
            self.add(i, f);
        }
        Ok(())
    }

    /// `block.load(fis, head)` and the recursion along `next`.
    fn load(&mut self, start: usize, mut head: usize, fis: &mut FontInputStream) -> FontResult<()> {
        let mut cur = Some(start);
        while let Some(i) = cur {
            match &self.b[i].kind {
                Kind::Table { .. } => self.load_table(i, head, fis)?,
                Kind::Font { .. } => self.load_font(i, fis)?,
                Kind::Ttc { .. } => {
                    self.load_ttc(i, fis)?;
                    if let Some(n) = self.b[i].next {
                        head = n;
                    }
                }
            }
            cur = self.b[i].next;
        }
        Ok(())
    }

    /// `buildFonts`: one font per font block, in list order.
    fn build_fonts(&self, start: usize, fingerprint: &[u8], s: Option<&str>, s2: Option<&str>) -> FontResult<Vec<Rc<OpenTypeFont>>> {
        let mut out = Vec::new();
        let mut cur = Some(start);
        while let Some(i) = cur {
            if let Kind::Font { tables, .. } = &self.b[i].kind {
                let mut map = HashMap::new();
                for &t in tables {
                    if let Kind::Table { tag, table } = &self.b[t].kind {
                        match table {
                            Some(table) => {
                                map.insert(*tag, table.clone());
                            }
                            None => {
                                map.remove(tag);
                            }
                        }
                    }
                }
                out.push(Rc::new(OpenTypeFont::new(&map, fingerprint.to_vec(), s.map(str::to_string), s2.map(str::to_string))?));
            }
            cur = self.b[i].next;
        }
        Ok(out)
    }
}

pub struct FontFactory;

impl FontFactory {
    pub fn load(fis: &mut FontInputStream) -> FontResult<Vec<Rc<OpenTypeFont>>> {
        Self::load_base14(fis, None, None)
    }

    pub fn load_base14(fis: &mut FontInputStream, s: Option<&str>, s2: Option<&str>) -> FontResult<Vec<Rc<OpenTypeFont>>> {
        let a = OTByteArray::from_font_byte_array(fis.read_font_byte_array(12)?);
        let mut blocks = Blocks { b: Vec::new() };
        let start = if a.getint32(0)? == Tag::misc_ttcf {
            blocks.push(0, 0, Kind::Ttc { header: Some(a) })
        } else {
            let len = a.get_size();
            blocks.push(0, len, Kind::Font { tables: Vec::new(), header: Some(a) })
        };
        blocks.load(start, start, fis)?;
        let fingerprint = fis.get_digest();
        blocks.build_fonts(start, &fingerprint, s, s2)
    }

    pub fn get_num_bytes_needed_to_identify() -> i32 {
        4
    }

    /// Java ORs the bytes in sign-extended.
    pub fn is_open_type(b: &[u8]) -> bool {
        let mut n: i32 = 0;
        for &x in b.iter().take(4) {
            n = n << 8 | x as i8 as i32;
        }
        n == Tag::misc_ttcf || n == Tag::misc_1_0 || n == Tag::misc_OTTO || n == Tag::misc_true
    }
}
