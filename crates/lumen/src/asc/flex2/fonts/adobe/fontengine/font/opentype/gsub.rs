//! Port of afe.jar `com.adobe.fontengine.font.opentype.Gsub` (the parts outside inline formatting).

use std::collections::BTreeMap;
use std::ops::Deref;

use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub_subsetter::GsubSubsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_subsetter::LookupTableSubsetter;
use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::{OTByteArray, OTByteArrayBuilder};
use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub const TAG_GSUB: i32 = 1196643650;

#[derive(Clone, Debug, Default)]
pub struct Gsub {
    pub base: LookupTable,
}

impl Deref for Gsub {
    type Target = LookupTable;
    fn deref(&self) -> &LookupTable {
        &self.base
    }
}

impl Gsub {
    pub fn new(font_byte_array: FontByteArray) -> FontResult<Gsub> {
        Ok(Gsub { base: LookupTable::new(font_byte_array) })
    }

    /// `lookups` is what `GsubHarvester.gatherPossibleLookups` returned.
    pub fn subset_and_stream(&self, subset: &dyn Subset, lookups: &BTreeMap<i32, Vec<i32>>, map: &mut BTreeMap<i32, OTByteArrayBuilder>, n: i32) -> FontResult<()> {
        let mut s = GsubSubsetter::new(&self.base, n);
        let builder = s.subset_and_stream(lookups, subset)?;
        map.insert(TAG_GSUB, builder);
        Ok(())
    }

    pub fn stream(&self, map: &mut BTreeMap<i32, OTByteArrayBuilder>) {
        map.insert(TAG_GSUB, OTByteArray::get_ot_byte_array_builder_instance_from(&self.data));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::asc::flex2::fonts::adobe::fontengine::font::font_byte_array::FontByteArray;
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::base::{Base, TAG_BASE};
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gdef::{Gdef, TAG_GDEF};
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos::{Gpos, TAG_GPOS};
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gpos_harvester::GposHarvester;
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub::{Gsub, TAG_GSUB};
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::gsub_harvester::GsubHarvester;
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::kern::{Kern, TAG_KERN};
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table_harvester::LookupMap;
    use crate::asc::flex2::fonts::adobe::fontengine::font::opentype::ot_byte_array::OTByteArrayBuilder;
    use crate::asc::flex2::fonts::adobe::fontengine::font::subset::Subset;
    use crate::asc::flex2::fonts::adobe::fontengine::font::subset_default_impl::SubsetDefaultImpl;
    use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    fn fba(b: &[u8]) -> FontByteArray {
        FontByteArray::from_bytes(b, b.len() as i32).unwrap()
    }

    fn lookups(m: &LookupMap) -> String {
        m.iter().map(|(k, v)| format!("{}:{}", k, v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(";")
    }

    fn gids(s: &dyn Subset) -> String {
        (0..s.get_num_glyphs()).map(|i| s.get_full_gid(i).to_string()).collect::<Vec<_>>().join(",")
    }

    #[derive(Default)]
    struct Tables {
        gsub: Option<Gsub>,
        gpos: Option<Gpos>,
        gdef: Option<Gdef>,
        base: Option<Base>,
        kern: Option<Kern>,
    }

    /// Replays one case as `OTFSWFFont4Description.streamFontData` runs it, returning the lines Java printed.
    fn replay(t: &Tables, n: i32, gids0: &[i32], out: &mut Vec<String>) -> FontResult<()> {
        let mut s = SubsetDefaultImpl::new(n, true);
        for &g in gids0 {
            s.get_subset_gid(g)?;
        }
        out.push(format!("GIDS0 {}", gids(&s)));
        let mut gl = None;
        if let Some(gsub) = &t.gsub {
            let l = GsubHarvester::new(gsub, n).gather_possible_lookups(&mut s)?;
            out.push(format!("GSUBL {}", lookups(&l)));
            out.push(format!("GIDS1 {}", gids(&s)));
            gl = Some(l);
        }
        let mut m: BTreeMap<i32, OTByteArrayBuilder> = BTreeMap::new();
        if let Some(gsub) = &t.gsub {
            gsub.subset_and_stream(&s, gl.as_ref().unwrap(), &mut m, n)?;
        }
        if let Some(kern) = &t.kern {
            kern.subset_and_stream_for_swf(&s, &mut m)?;
        }
        if let Some(gpos) = &t.gpos {
            let pl = GposHarvester::new(gpos, n).gather_possible_lookups(&mut s)?;
            out.push(format!("GPOSL {}", lookups(&pl)));
            let has_kern = m.contains_key(&TAG_KERN);
            gpos.subset_and_stream(&s, &pl, &mut m, n, has_kern)?;
        }
        if let Some(gdef) = &t.gdef {
            gdef.subset_and_stream(&s, n, &mut m)?;
        }
        if let Some(base) = &t.base {
            base.subset_and_stream(&s, &mut m)?;
        }
        out.push(format!("GIDS2 {}", gids(&s)));
        for (k, v) in &m {
            let b = v.base.byte_array.as_slice();
            out.push(format!("OUT {} {}", k, b.iter().map(|x| format!("{:02x}", x)).collect::<String>()));
        }
        Ok(())
    }

    /// The `Q` lines of `LayoutDump.queries`.
    fn query(t: &Tables, n: i32, k: &str) -> FontResult<String> {
        let p: Vec<&str> = k.split(' ').collect();
        let feats = [0, tag("liga"), tag("kern"), tag("mark"), tag("ccmp"), tag("smcp"), tag("ital"), tag("size"), tag("locl")];
        let lt = |i: &str| -> &crate::asc::flex2::fonts::adobe::fontengine::font::opentype::lookup_table::LookupTable {
            if i == "0" {
                &t.gsub.as_ref().unwrap().base
            } else {
                &t.gpos.as_ref().unwrap().base
            }
        };
        Ok(match p[0] {
            "fp" => lt(p[1]).feature_is_present(p[2].parse().unwrap())?.to_string(),
            "rft" => lt(p[1]).resolve_feature_tag(p[2].parse().unwrap(), p[3].parse().unwrap(), &feats)?.iter().map(|a| format!("[{}]", a.iter().map(|x| format!("{} ", x)).collect::<String>())).collect(),
            "osd" => match t.gpos.as_ref().unwrap().get_optical_size_data(p[1] == "true")? {
                None => "null".into(),
                Some(d) => format!("{} {} {} {} {}", d.design_size, d.subfamily_id, d.name_id, d.min_size, d.max_size),
            },
            "bp" => t.base.as_ref().unwrap().get_baseline_position(p[1] == "0", p[2].parse().unwrap(), p[3].parse().unwrap())?.to_string(),
            "kv" => {
                let l: i32 = p[1].parse().unwrap();
                let mut s = String::new();
                for r in 0..n.min(120) {
                    let v = t.kern.as_ref().unwrap().get_kern_vector(l, r)?;
                    if v != [0, 0] {
                        s += &format!("{}:{},{} ", r, v[0], v[1]);
                    }
                }
                s
            }
            "gc" => {
                let mut s = String::new();
                for g in 0..n.min(5000) {
                    s += &t.gdef.as_ref().unwrap().get_glyph_class(g)?.to_string();
                }
                s
            }
            "mac" => {
                let mut s = String::new();
                for g in 0..n.min(5000) {
                    s += &format!("{},", t.gdef.as_ref().unwrap().get_mark_attach_class(g)?);
                }
                s
            }
            _ => panic!("query {}", k),
        })
    }

    fn tag(s: &str) -> i32 {
        let b = s.as_bytes();
        (b[0] as i32) << 24 | (b[1] as i32) << 16 | (b[2] as i32) << 8 | b[3] as i32
    }

    /// Replays the dumps of `LayoutDump.java` (one file per font face).
    #[test]
    fn layout_tables_match_java() {
        let dir = std::env::var("LAYOUT_DUMPS").unwrap_or_else(|_| "S:/Desktop/Projects/Rust-FFDec/target/fonts_ws/layout/dumps".into());
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.file_name().unwrap().to_string_lossy().contains('_')).collect();
        files.sort();
        let (mut cases, mut ok, mut tables_out, mut bytes_out, mut fails, mut queries) = (0, 0, 0usize, 0usize, Vec::new(), 0);
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let mut lines = text.lines().peekable();
            let head: Vec<&str> = lines.next().unwrap().split(' ').collect();
            let n: i32 = head[3].parse().unwrap();
            let mut t = Tables::default();
            while let Some(l) = lines.peek() {
                if !l.starts_with("TABLE ") {
                    break;
                }
                let p: Vec<&str> = lines.next().unwrap().split(' ').collect();
                let b = unhex(p[2]);
                let tag: i32 = p[1].parse().unwrap();
                match tag {
                    TAG_GSUB => {
                        let g = Gsub::new(fba(&b)).unwrap();
                        let mut m = BTreeMap::new();
                        g.stream(&mut m);
                        assert_eq!(m[&TAG_GSUB].base.byte_array.as_slice(), &b[..]);
                        t.gsub = Some(g);
                    }
                    TAG_GPOS => t.gpos = Some(Gpos::new(fba(&b)).unwrap()),
                    TAG_GDEF => t.gdef = Some(Gdef::new(fba(&b)).unwrap()),
                    TAG_BASE => t.base = Some(Base::new(fba(&b)).unwrap()),
                    TAG_KERN => t.kern = Some(Kern::new(fba(&b)).unwrap()),
                    _ => panic!("tag {}", tag),
                }
            }
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            while let Some(l) = lines.peek() {
                let Some(q) = l.strip_prefix("Q ") else { break };
                let (k, want) = q.split_once(" = ").unwrap();
                let got = match query(&t, n, k) {
                    Ok(v) => v,
                    Err(e) => format!("ERR {}", e.to_java_string()),
                };
                queries += 1;
                if got != want {
                    fails.push(format!("{} query {}\n  java {}\n  rust {}", name, k, want, got));
                }
                lines.next();
            }
            let mut case = String::new();
            let mut expected: Vec<String> = Vec::new();
            let mut flush = |case: &str, expected: &mut Vec<String>| {
                if case.is_empty() {
                    return;
                }
                cases += 1;
                let gids0: Vec<i32> = expected[0][6..].split(',').filter(|x| !x.is_empty()).map(|x| x.parse().unwrap()).collect();
                let mut got = Vec::new();
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| replay(&t, n, &gids0, &mut got)));
                match r {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => got.push(format!("ERR {}", e.to_java_string())),
                    Err(p) => got.push(format!("ERR java.lang.RuntimeException: {}", p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default())),
                }
                if got == *expected {
                    ok += 1;
                    for l in expected.iter().filter(|l| l.starts_with("OUT ")) {
                        tables_out += 1;
                        bytes_out += (l.len() - l.rfind(' ').unwrap() - 1) / 2;
                    }
                } else {
                    let i = got.iter().zip(expected.iter()).position(|(a, b)| a != b).unwrap_or(got.len().min(expected.len()));
                    let short = |s: Option<&String>| s.map(|s| s.chars().take(300).collect::<String>()).unwrap_or_default();
                    fails.push(format!("{} {}: line {}\n  java {}\n  rust {}", name, case, i, short(expected.get(i)), short(got.get(i))));
                }
                expected.clear();
            };
            for l in lines {
                if let Some(c) = l.strip_prefix("CASE ") {
                    flush(&case, &mut expected);
                    case = c.to_string();
                } else if l == "END" {
                    flush(&case, &mut expected);
                    case.clear();
                } else {
                    expected.push(l.to_string());
                }
            }
        }
        eprintln!("layout: {}/{} cases match, {} tables / {} bytes written; {} queries", ok, cases, tables_out, bytes_out, queries);
        for f in &fails {
            eprintln!("{}", f);
        }
        assert!(fails.is_empty());
    }
}
