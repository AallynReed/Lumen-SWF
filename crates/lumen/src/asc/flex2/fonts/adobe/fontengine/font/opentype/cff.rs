//! Port of afe.jar `com.adobe.fontengine.font.opentype.Cff`.

use std::collections::BTreeMap;
use std::io::Write;

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::{AnyCFFFont, CFFFontImpl};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::font_factory::FontFactory;
use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArrayBuilder, OTByteArrayBuilderOutputStreamAdaptor};
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font::rect::Rect;
use crate::asc::flex2::fonts::adobe::fontengine::font::ros::ROS;
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::{FontEngineException, FontResult};

/// The table tag as a signed int.
pub const TAG: i32 = 1128678944;

#[derive(Debug)]
pub struct Cff {
    pub cff_font: AnyCFFFont,
}

impl Cff {
    pub fn new(a: FontByteArray) -> FontResult<Cff> {
        let mut fonts = FontFactory::load(a)?;
        if fonts.len() != 1 || fonts[0].is_none() {
            return Err(FontEngineException::invalid_font("CFFFontSet not appropriate for CFF table"));
        }
        Ok(Cff { cff_font: fonts.remove(0).unwrap() })
    }

    pub fn get_cff_font(&self) -> &AnyCFFFont {
        &self.cff_font
    }

    pub fn get_glyph_name(&self, n: i32) -> FontResult<Option<String>> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_glyph_name(n),
            AnyCFFFont::CIDKeyed(f) => Ok(f.get_glyph_name(n)),
        }
    }

    pub fn get_ros(&self) -> Option<ROS> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(_) => None,
            AnyCFFFont::CIDKeyed(f) => f.get_ros(),
        }
    }

    pub fn get_glyph_cid(&self, n: i32) -> FontResult<i32> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => Ok(f.get_glyph_cid(n)),
            AnyCFFFont::CIDKeyed(f) => f.get_glyph_cid(n),
        }
    }

    pub fn get_outline(&self, n: i32, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_glyph_outline(n, oc),
            AnyCFFFont::CIDKeyed(f) => f.get_glyph_outline(n, oc),
        }
    }

    pub fn get_font_bbox(&self) -> Option<Rect> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_font_bbox(),
            AnyCFFFont::CIDKeyed(f) => f.get_font_bbox(),
        }
    }

    pub fn get_embedding_permission(&self, bl: bool) -> Permission {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_embedding_permission(bl),
            AnyCFFFont::CIDKeyed(f) => f.get_embedding_permission(bl),
        }
    }

    /// `getCFFFont().getUnitsPerEmX()`.
    pub fn get_units_per_em_x(&self) -> f64 {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_units_per_em_x(),
            AnyCFFFont::CIDKeyed(f) => f.get_units_per_em_x(),
        }
    }

    /// `getCFFFont().getUnitsPerEmY()`.
    pub fn get_units_per_em_y(&self) -> f64 {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_units_per_em_y(),
            AnyCFFFont::CIDKeyed(f) => f.get_units_per_em_y(),
        }
    }

    /// `getCFFFont().getHorizontalAdvance(gid)`.
    pub fn get_horizontal_advance(&self, n: i32) -> FontResult<f64> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.get_horizontal_advance(n),
            AnyCFFFont::CIDKeyed(f) => f.get_horizontal_advance(n),
        }
    }

    /// `subsetAndStream(Subset, OutputStream, boolean, Integer)`.
    pub fn subset_and_stream_out(&self, subset: &dyn Subset, out: &mut dyn Write, bl: bool, n: Option<i32>) -> FontResult<()> {
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.subset_and_stream4(subset, out, bl, n),
            AnyCFFFont::CIDKeyed(f) => f.subset_and_stream4(subset, out, bl, n),
        }
    }

    /// `subsetAndStream(Subset, Map, boolean, Integer, boolean)`.
    pub fn subset_and_stream(&self, subset: &dyn Subset, map: &mut BTreeMap<i32, OTByteArrayBuilder>, bl: bool, n: Option<i32>, bl2: bool) -> FontResult<()> {
        let mut a = OTByteArrayBuilderOutputStreamAdaptor::new();
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.subset_and_stream5(subset, &mut a, bl, n, bl2)?,
            AnyCFFFont::CIDKeyed(f) => f.subset_and_stream5(subset, &mut a, bl, n, bl2)?,
        }
        map.insert(TAG, a.builder);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>, n: Option<i32>) -> FontResult<()> {
        let mut a = OTByteArrayBuilderOutputStreamAdaptor::new();
        match &self.cff_font {
            AnyCFFFont::NameKeyed(f) => f.stream(&mut a, n)?,
            AnyCFFFont::CIDKeyed(f) => f.stream(&mut a, n)?,
        }
        map.insert(TAG, a.builder);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_cstring_generator::dump_tests::{dump_files, first_diff, skip_glyf, skip_outline, Rd};
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_font::create_subset_charstring_index;
    use crate::asc::flex2::fonts::adobe::fontengine::font::cff::cff_subrize::CFFSubrize;
    use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
    use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;

    /// Writes consumer calls in the harness's format.
    struct Rec {
        out: Vec<u8>,
    }

    impl Rec {
        fn d(&mut self, v: &[f64]) {
            for x in v {
                self.out.extend_from_slice(&x.to_bits().to_be_bytes());
            }
        }
    }

    impl OutlineConsumer for Rec {
        fn set_matrix(&mut self, m: &Matrix) {
            self.out.push(2);
            self.d(&[m.a, m.b, m.c, m.d, m.tx, m.ty]);
        }
        fn moveto(&mut self, x: f64, y: f64) {
            self.out.push(3);
            self.d(&[x, y]);
        }
        fn lineto(&mut self, x: f64, y: f64) {
            self.out.push(4);
            self.d(&[x, y]);
        }
        fn curveto(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
            self.out.push(5);
            self.d(&[a, b, c, d, e, f]);
        }
        fn curveto_quad(&mut self, a: f64, b: f64, c: f64, d: f64) {
            self.out.push(6);
            self.d(&[a, b, c, d]);
        }
        fn endchar(&mut self) {
            self.out.push(7);
        }
    }

    fn media_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(std::env::var("CFF_MEDIA_DIR").unwrap_or_else(|_| {
            "C:/Users/Aallyn/AppData/Local/Temp/claude/S--Desktop-Projects-Rust-FFDec/918e2469-beb7-4861-9c86-13b9dac07e23/scratchpad/fonts/media".into()
        }))
    }

    /// The `CFF ` table of an sfnt file.
    fn cff_table(file: &[u8]) -> Option<Vec<u8>> {
        let be16 = |p: usize| u16::from_be_bytes([file[p], file[p + 1]]) as usize;
        let be32 = |p: usize| u32::from_be_bytes([file[p], file[p + 1], file[p + 2], file[p + 3]]) as usize;
        let n = be16(4);
        for i in 0..n {
            let r = 12 + 16 * i;
            if &file[r..r + 4] == b"CFF " {
                let (off, len) = (be32(r + 8), be32(r + 12));
                return Some(file[off..off + len].to_vec());
            }
        }
        None
    }

    fn table_bytes(m: &BTreeMap<i32, OTByteArrayBuilder>) -> Vec<u8> {
        m[&TAG].base.byte_array.as_slice().to_vec()
    }

    #[test]
    fn cff_fonts_match_java() {
        let (mut cases, mut bytes, mut glyphs) = (0, 0, 0);
        for p in dump_files() {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let font_file = name.rsplit_once('_').map(|(a, _)| a).unwrap_or(&name);
            let otf = format!("{}.otf", font_file);
            let cff = std::fs::read(media_dir().join(&otf)).or_else(|_| std::fs::read(media_dir().join("../cff/media2").join(&otf))).ok().map(|file| {
                let t = cff_table(&file).unwrap();
                Cff::new(FontByteArray::from_bytes(&t, t.len() as i32).unwrap()).unwrap()
            });
            let data = std::fs::read(&p).unwrap();
            let mut r = Rd { d: &data, p: 4 };
            loop {
                match r.u8() {
                    b'K' => {
                        let c = r.str().unwrap();
                        let mut m = BTreeMap::new();
                        let kind = r.u8();
                        if kind == b'G' {
                            skip_glyf(&mut r);
                            continue;
                        }
                        let cff = cff.as_ref().unwrap();
                        match kind {
                            b'C' => {
                                let n = r.i();
                                let ng = match &cff.cff_font {
                                    AnyCFFFont::NameKeyed(f) => f.get_num_glyphs(),
                                    AnyCFFFont::CIDKeyed(f) => f.get_num_glyphs(),
                                };
                                let mut s = SubsetDefaultImpl::new(ng, true);
                                for _ in 0..n {
                                    let g = r.i();
                                    s.get_subset_gid(g).unwrap();
                                }
                                let steps = r.i();
                                if let AnyCFFFont::NameKeyed(nk) = &cff.cff_font {
                                    let ck = nk.to_cid(&s).unwrap();
                                    let sdi = SubsetDefaultImpl::new(s.get_num_glyphs(), false);
                                    let bl2 = ck.char_strings.data.get_size() <= 614400;
                                    let cs = create_subset_charstring_index(&ck, &sdi, bl2).unwrap();
                                    let fds: Vec<u8> = (0..sdi.get_num_glyphs()).map(|k| ck.get_fd_for_glyph(sdi.get_full_gid(k)).unwrap() as u8).collect();
                                    let mut sz = CFFSubrize::new();
                                    let cs2 = sz.subrize_fd(cs.clone(), Some(fds));
                                    let got: Vec<Option<Vec<u8>>> = [Some(&ck.char_strings), Some(&cs), Some(&cs2), sz.get_g_subrs().as_ref(), sz.get_l_subrs_fd(0).as_ref()]
                                        .iter()
                                        .map(|c| c.map(|c| c.data.base.get_bytes(c.offset, c.size())))
                                        .chain(std::iter::once(Some(sz.debug_subr_table())))
                                        .chain(std::iter::once(Some(sz.debug_link_table())))
                                        .collect();
                                    let exps: Vec<Option<Vec<u8>>> = (0..steps).map(|_| r.bytes()).collect();
                                    if exps.len() > 6 && got[6] != exps[6] {
                                        if let Ok(d) = std::env::var("CFF_DEBUG_OUT") {
                                            std::fs::write(format!("{}/got_l.bin", d), got[6].as_ref().unwrap()).unwrap();
                                            std::fs::write(format!("{}/exp_l.bin", d), exps[6].as_ref().unwrap()).unwrap();
                                        }
                                        panic!("{} {}: links differ", name, c);
                                    }
                                    if exps.len() > 5 && got[5] != exps[5] {
                                        let (g, e) = (got[5].as_ref().unwrap(), exps[5].as_ref().unwrap());
                                        if let Ok(d) = std::env::var("CFF_DEBUG_OUT") {
                                            std::fs::write(format!("{}/got_t.bin", d), g).unwrap();
                                            std::fs::write(format!("{}/exp_t.bin", d), e).unwrap();
                                        }
                                        let i = g.chunks(23).zip(e.chunks(23)).position(|(a, b)| a != b).unwrap_or(0);
                                        panic!("{} {}: mSubrs differ ({} vs {} entries), first at {}: {:?} vs {:?}", name, c, g.len() / 23, e.len() / 23, i, g.chunks(23).nth(i), e.chunks(23).nth(i));
                                    }
                                    for (k, (g, e)) in got.iter().zip(&exps).enumerate() {
                                        assert!(g == e, "{} {}: CID step {} {}", name, c, k, first_diff(g.as_deref().unwrap_or(&[]), e.as_deref().unwrap_or(&[])));
                                    }
                                } else {
                                    for _ in 0..steps {
                                        r.bytes();
                                    }
                                }
                                cff.subset_and_stream(&s, &mut m, false, None, true).unwrap();
                            }
                            b'S' => cff.stream(&mut m, None).unwrap(),
                            x => panic!("{}: unexpected case kind {}", name, x),
                        }
                        let exp = r.bytes().unwrap();
                        let got = table_bytes(&m);
                        if got != exp {
                            if let Ok(d) = std::env::var("CFF_DEBUG_OUT") {
                                std::fs::write(format!("{}/got.bin", d), &got).unwrap();
                                std::fs::write(format!("{}/exp.bin", d), &exp).unwrap();
                            }
                        }
                        assert!(got == exp, "{} {}: {}", name, c, first_diff(&got, &exp));
                        cases += 1;
                        bytes += got.len();
                    }
                    b'O' => {
                        let cff = cff.as_ref().unwrap();
                        let n = r.i();
                        for g in 0..n {
                            let start = r.p;
                            skip_outline(&mut r);
                            let exp = &data[start..r.p];
                            let mut rec = Rec { out: Vec::new() };
                            match cff.get_outline(g, &mut rec) {
                                Ok(()) => rec.out.push(0),
                                Err(e) => {
                                    rec.out.push(255);
                                    let s: Vec<u16> = e.to_java_string().encode_utf16().collect();
                                    rec.out.extend_from_slice(&(s.len() as i32).to_be_bytes());
                                    for u in s {
                                        rec.out.extend_from_slice(&u.to_be_bytes());
                                    }
                                }
                            }
                            assert!(rec.out == exp, "{}: outline of glyph {} {}", name, g, first_diff(&rec.out, exp));
                            glyphs += 1;
                        }
                    }
                    b'R' => {
                        let blob = r.bytes().unwrap();
                        let fonts = FontFactory::load(FontByteArray::from_bytes(&blob, blob.len() as i32).unwrap()).unwrap();
                        let ck = match &fonts[0] {
                            Some(AnyCFFFont::CIDKeyed(f)) => f,
                            _ => panic!("{}: not CID-keyed", name),
                        };
                        let n = r.i();
                        for g in 0..n {
                            let start = r.p;
                            skip_outline(&mut r);
                            let exp = &data[start..r.p];
                            let mut rec = Rec { out: Vec::new() };
                            ck.get_glyph_outline(g, &mut rec).unwrap();
                            rec.out.push(0);
                            assert!(rec.out == exp, "{}: raw outline of glyph {} {}", name, g, first_diff(&rec.out, exp));
                            glyphs += 1;
                        }
                        let k = r.i();
                        let mut s = SubsetDefaultImpl::new(n, true);
                        for _ in 0..k {
                            let g = r.i();
                            s.get_subset_gid(g).unwrap();
                        }
                        for (bl, bl2) in [(false, true), (true, true), (true, false)] {
                            let mut out = Vec::new();
                            ck.subset_and_stream5(&s, &mut out, bl, None, bl2).unwrap();
                            let exp = r.bytes().unwrap();
                            assert!(out == exp, "{}: raw subset {} {} {}", name, bl, bl2, first_diff(&out, &exp));
                            cases += 1;
                            bytes += out.len();
                        }
                        let mut out = Vec::new();
                        ck.stream(&mut out, None).unwrap();
                        let exp = r.bytes().unwrap();
                        assert!(out == exp, "{}: raw stream {}", name, first_diff(&out, &exp));
                        cases += 1;
                        bytes += out.len();
                    }
                    b'E' => break,
                    x => panic!("bad record {} in {}", x, name),
                }
            }
        }
        eprintln!("CFF fonts: {} cases, {} bytes, {} glyph outlines matched", cases, bytes, glyphs);
    }
}
