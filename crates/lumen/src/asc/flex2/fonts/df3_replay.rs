//! Replays the `SWFFontDescription` calls `integ\harness\Df3Dump.java` recorded through `AFEFontFace`, `FontBuilder`
//! (with the flashtype zones) and `TagEncoder`, and compares the DefineFont3 + DefineFontAlignZones + DefineFontName
//! bytes with Java's.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::flash::swf::builder::font_builder::FontBuilder;
use crate::asc::flex2::flash::swf::tag_encoder::TagEncoder;
use crate::asc::flex2::flash::swf::tags::{codes, Body};
use crate::asc::flex2::fonts::adobe::fontengine::font::swf_font_description::SWFFontDescription;
use crate::asc::flex2::fonts::adobe::afe_font_manager::{AFEFontFace, AFEFontManager};
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::permission::Permission;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;
use crate::asc::flex2::fonts::font_description::JUrl;
use crate::asc::flex2::fonts::font_manager::FontManager;

const DIR: &str = r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\integ\df3";

struct Replay(HashMap<String, String>);

fn s(v: &str) -> Option<String> {
    if v == "null" {
        return None;
    }
    let h = &v[1..];
    let u: Vec<u16> = (0..h.len() / 4).map(|i| u16::from_str_radix(&h[4 * i..4 * i + 4], 16).unwrap()).collect();
    Some(String::from_utf16(&u).unwrap())
}

fn d(v: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(v, 16).unwrap())
}

impl Replay {
    fn get(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("Rust asked {k}, Java did not"))
    }
}

impl SWFFontDescription for Replay {
    fn get_outline(&self, c: u16, oc: &mut dyn OutlineConsumer) -> FontResult<()> {
        let ops = self.get(&format!("O {c}")).to_string();
        let t: Vec<&str> = ops.split_whitespace().collect();
        let mut i = 0;
        while i < t.len() {
            match t[i] {
                "m" => {
                    oc.set_matrix(&Matrix::new(d(t[i + 1]), d(t[i + 2]), d(t[i + 3]), d(t[i + 4]), d(t[i + 5]), d(t[i + 6])));
                    i += 7;
                }
                "M" => {
                    oc.moveto(d(t[i + 1]), d(t[i + 2]));
                    i += 3;
                }
                "L" => {
                    oc.lineto(d(t[i + 1]), d(t[i + 2]));
                    i += 3;
                }
                "C" => {
                    oc.curveto(d(t[i + 1]), d(t[i + 2]), d(t[i + 3]), d(t[i + 4]), d(t[i + 5]), d(t[i + 6]));
                    i += 7;
                }
                "Q" => {
                    oc.curveto_quad(d(t[i + 1]), d(t[i + 2]), d(t[i + 3]), d(t[i + 4]));
                    i += 5;
                }
                "E" => {
                    oc.endchar();
                    i += 1;
                }
                x => panic!("op {x}"),
            }
        }
        Ok(())
    }
    fn get_horizontal_advance(&self, c: u16) -> FontResult<f64> {
        Ok(d(self.get(&format!("getHorizontalAdvance {c}"))))
    }
    fn can_display(&self, c: u16) -> FontResult<bool> {
        Ok(self.get(&format!("canDisplay {c}")) == "true")
    }
    fn get_permissions(&self) -> FontResult<Permission> {
        Ok(match self.get("getPermissions") {
            "EDITABLE" => Permission::Editable,
            "INSTALLABLE" => Permission::Installable,
            "PREVIEW_AND_PRINT" => Permission::PreviewAndPrint,
            "RESTRICTED" => Permission::Restricted,
            _ => Permission::IllegalValue,
        })
    }
    fn get_family(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getFamily")))
    }
    fn get_sub_family(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getSubFamily")))
    }
    fn get_ascent(&self) -> FontResult<f64> {
        Ok(d(self.get("getAscent")))
    }
    fn get_descent(&self) -> FontResult<f64> {
        Ok(d(self.get("getDescent")))
    }
    fn get_line_gap(&self) -> FontResult<f64> {
        Ok(d(self.get("getLineGap")))
    }
    fn get_first_char(&self) -> FontResult<i32> {
        Ok(self.get("getFirstChar").parse().unwrap())
    }
    fn get_last_char(&self) -> FontResult<i32> {
        Ok(self.get("getLastChar").parse().unwrap())
    }
    fn get_num_glyphs(&self) -> FontResult<i32> {
        Ok(self.get("getNumGlyphs").parse().unwrap())
    }
    fn is_bold(&self) -> FontResult<bool> {
        Ok(self.get("isBold") == "true")
    }
    fn is_italic(&self) -> FontResult<bool> {
        Ok(self.get("isItalic") == "true")
    }
    fn get_em_scale(&self) -> FontResult<f64> {
        Ok(d(self.get("getEmScale")))
    }
    fn get_copyright(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getCopyright")))
    }
    fn get_trademark(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getTrademark")))
    }
    fn get_postscript_name(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getPostscriptName")))
    }
    fn get_full_name(&self) -> FontResult<Option<String>> {
        Ok(s(self.get("getFullName")))
    }
}

fn parse(text: &str) -> (String, HashMap<String, String>) {
    let mut lines = text.lines();
    let spec = lines.next().unwrap()[2..].to_string();
    let mut map = HashMap::new();
    for l in lines {
        if let Some(rest) = l.strip_prefix("O ") {
            let (c, ops) = rest.split_once(' ').unwrap_or((rest, ""));
            map.insert(format!("O {c}"), ops.to_string());
        } else if let Some((k, v)) = l.rsplit_once(' ') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    (spec, map)
}

#[test]
fn df3_replay() {
    if std::env::var_os("LUMEN_FONT_REPLAY").is_none() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(DIR) else { return };
    let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".rec")).collect();
    names.sort();
    let (mut ok, mut bad) = (0, Vec::new());
    for n in &names {
        let (spec, map) = parse(&std::fs::read_to_string(format!("{DIR}\\{n}")).unwrap());
        let p: Vec<&str> = spec.split('|').collect();
        let url = s(&map["url"]).unwrap();
        let replay: Rc<dyn SWFFontDescription> = Rc::new(Replay(map));
        let face = AFEFontFace::new_instance(1000, replay, JUrl { external_form: url, path: p[0].to_string() }, true).unwrap_or_else(|e| panic!("{e:?}"));
        face.cached.style.set(p[2].parse().unwrap());
        let mut b = FontBuilder::new(codes::DEFINE_FONT3, Rc::new(face), p[5].to_string(), p[3] == "true").unwrap_or_else(|e| panic!("{e:?}"));
        match AFEFontManager::new().get_unicode_ranges(if p[4] == "-" { None } else { Some(p[4]) }).unwrap() {
            Some(rs) => {
                for r in rs {
                    b.add_charset(r[0], r[1] - r[0] + 1).unwrap_or_else(|e| panic!("{e:?}"));
                }
            }
            None => b.add_all_chars().unwrap_or_else(|e| panic!("{e:?}")),
        }
        let tag = b.build().unwrap_or_else(|e| panic!("{e:?}"));
        let mut te = TagEncoder::new(10, false);
        te.visit(&tag);
        let (zones, license) = match &tag.borrow().body {
            Body::DefineFont2(f) => (f.zones.clone(), f.license.clone()),
            _ => unreachable!(),
        };
        if let Some(z) = zones {
            te.visit(&z);
        }
        if let Some(l) = license {
            te.visit(&l);
        }
        let got = te.take_bytes();
        let want = std::fs::read(format!("{DIR}\\{}", n.replace(".rec", ".tags"))).unwrap();
        if got == want {
            ok += 1;
        } else {
            let at = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(got.len().min(want.len()));
            bad.push(format!("{n} ({spec}): rust {} java {} bytes, first diff at {at}", got.len(), want.len()));
        }
    }
    eprintln!("df3 replay: {ok}/{} identical", names.len());
    for b in &bad {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty());
}
