//! Port of flex-fontkit.jar `flash.fonts.flashtype.FlashTypeZoneRecordBuilder`.

use crate::asc::flex2::fonts::adobe::flashtype::adf_algn_zones::{adf_detect_algn_zones, adf_init_algn_zone_detection, AlgnZoneState};
use crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::{ADFPath, ADFPenCmd};
use crate::asc::flex2::fonts::adobe::flashtype::flex_adf_data::FlexADFData;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub const ZONE_STATE: &str = "ZONESTATE";

/// A glyph's SWF shape record. Every StyleChangeRecord counts as a moveTo (Java reads its move deltas, 0 when it has none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeRecordIn {
    StyleChange { move_delta_x: i32, move_delta_y: i32 },
    StraightEdge { delta_x: i32, delta_y: i32 },
    CurvedEdge { control_delta_x: i32, control_delta_y: i32, anchor_delta_x: i32, anchor_delta_y: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneRecordOut {
    pub num_zone_data: i32,
    pub zone_data: Vec<i64>,
    pub zone_mask: i32,
}

fn empty_record() -> ZoneRecordOut {
    ZoneRecordOut { num_zone_data: 2, zone_data: vec![0, 0], zone_mask: 0 }
}

/// createZoneRecord; `path` is get_font_outline of the glyph (not used for ' '). Err(()) where Java throws
/// "Could not create ZoneTable for character ...".
pub fn create_zone_record(state: &mut AlgnZoneState, c: u16, path: Option<ADFPath>) -> Result<ZoneRecordOut, ()> {
    if c == ' ' as u16 {
        return Ok(empty_record());
    }
    let mut path = path.ok_or(())?;
    let r = catch_unwind(AssertUnwindSafe(|| adf_detect_algn_zones(Some(&mut *state), Some(&mut path))));
    if r.is_err() {
        return Err(());
    }
    Ok(match path.algn_zones {
        None => empty_record(),
        Some(z) => ZoneRecordOut { num_zone_data: z.len() as i32, zone_data: z, zone_mask: path.algn_zones_mask },
    })
}

/// getFontOutline over a glyph's shape records (twips).
pub fn get_font_outline(records: &[ShapeRecordIn], c: u16) -> ADFPath {
    let mut d = FlexADFData::default();
    let p = &mut d.m_path;
    p.font_units_per_em = 20480.0;
    p.char_code = c;
    let mut f = 10000.0f32;
    let mut f2 = 10000.0f32;
    let mut f3 = -10000.0f32;
    let mut f4 = -10000.0f32;
    let mut n = 0i32;
    let mut n2 = 0i64;
    let mut n3 = 0i32;
    let mut n4 = 0i32;
    let mut bl = false;
    let mut n5 = 0i32;
    let mut n6 = 0i32;
    let mut cmds: Vec<ADFPenCmd> = Vec::with_capacity(records.len() * 2);
    let line = |x: i32, y: i32| ADFPenCmd { op_code: 1, x: x as f32, y: y.wrapping_mul(-1) as f32, cx: 0.0, cy: 0.0 };
    for r in records {
        match *r {
            ShapeRecordIn::StyleChange { move_delta_x, move_delta_y } => {
                if bl && n3 != n5 && n4 != n6 {
                    cmds.push(line(n5, n6));
                    n2 += 1;
                }
                n3 = move_delta_x;
                n4 = move_delta_y;
                bl = true;
                n5 = n3;
                n6 = n4;
                cmds.push(ADFPenCmd { op_code: 0, x: n3 as f32, y: n4.wrapping_mul(-1) as f32, cx: 0.0, cy: 0.0 });
                n += 1;
                n2 += 1;
            }
            ShapeRecordIn::StraightEdge { delta_x, delta_y } => {
                n3 = n3.wrapping_add(delta_x);
                n4 = n4.wrapping_add(delta_y);
                cmds.push(line(n3, n4));
                n2 += 1;
            }
            ShapeRecordIn::CurvedEdge { control_delta_x, control_delta_y, anchor_delta_x, anchor_delta_y } => {
                let n7 = n3.wrapping_add(control_delta_x);
                let n8 = n4.wrapping_add(control_delta_y);
                n3 = n7.wrapping_add(anchor_delta_x);
                n4 = n8.wrapping_add(anchor_delta_y);
                cmds.push(ADFPenCmd { op_code: 2, x: n3 as f32, y: n4.wrapping_mul(-1) as f32, cx: n7 as f32, cy: n8.wrapping_mul(-1) as f32 });
                n2 += 1;
                if (n7 as f32) < f {
                    f = n7 as f32;
                }
                if n7 as f32 > f3 {
                    f3 = n7 as f32;
                }
                if (n8 as f32) < f2 {
                    f2 = n8 as f32;
                }
                if n8 as f32 > f4 {
                    f4 = n8 as f32;
                }
            }
        }
        if (n3 as f32) < f {
            f = n3 as f32;
        }
        if n3 as f32 > f3 {
            f3 = n3 as f32;
        }
        if (n4 as f32) < f2 {
            f2 = n4 as f32;
        }
        if n4 as f32 > f4 {
            f4 = n4 as f32;
        }
    }
    if bl && n3 != n5 && n4 != n6 {
        cmds.push(line(n5, n6));
        n2 += 1;
    }
    p.pen_cmds = cmds;
    p.glyph_min_x = f;
    p.glyph_min_y = f4 * -1.0;
    p.glyph_max_x = f3;
    p.glyph_max_y = f2 * -1.0;
    p.num_contours = n;
    p.num_pen_cmds = n2;
    d.m_path
}

/// initializeAlignZoneState: Err(()) where Java throws (the caller builds "Could not initialize FlashType for font ...").
pub fn initialize_align_zone_state(get_outline: &mut dyn FnMut(u16) -> ADFPath) -> Result<AlgnZoneState, ()> {
    catch_unwind(AssertUnwindSafe(|| adf_init_algn_zone_detection(get_outline))).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const DUMP: &str = r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\flashtype\dump.txt";

    fn parse_recs(toks: &[&str]) -> Vec<ShapeRecordIn> {
        let mut v = Vec::new();
        for t in toks {
            let p: Vec<&str> = t.split(',').collect();
            let n = |i: usize| p[i].parse::<i32>().unwrap();
            match p[0] {
                "s" => v.push(ShapeRecordIn::StyleChange { move_delta_x: n(1), move_delta_y: n(2) }),
                "e" => v.push(ShapeRecordIn::StraightEdge { delta_x: n(1), delta_y: n(2) }),
                "c" => v.push(ShapeRecordIn::CurvedEdge { control_delta_x: n(1), control_delta_y: n(2), anchor_delta_x: n(3), anchor_delta_y: n(4) }),
                "n" => {}
                o => panic!("bad record {o}"),
            }
        }
        v
    }

    fn fb(f: f32) -> String {
        format!("{:x}", f.to_bits())
    }

    fn path_str(p: &ADFPath) -> String {
        let mut s = format!(
            "{} {} {} {} {} {} {} {}",
            fb(p.glyph_min_x),
            fb(p.glyph_min_y),
            fb(p.glyph_max_x),
            fb(p.glyph_max_y),
            fb(p.font_units_per_em),
            p.char_code,
            p.num_contours,
            p.num_pen_cmds
        );
        for c in &p.pen_cmds {
            s += &format!(" {}:{}:{}:{}:{}", c.op_code, fb(c.x), fb(c.y), fb(c.cx), fb(c.cy));
        }
        s
    }

    fn adf_hash(p: &ADFPath) -> String {
        use crate::asc::flex2::fonts::adobe::flashtype::adf::{adf_generate_adf, adf_reconstruct_dist_abs};
        use crate::asc::flex2::fonts::adobe::flashtype::adf_type_system::ADFGenAttrs;
        let mut a = ADFGenAttrs { max_level: 5, max_error: 2.0E-4, dist_eps: 0.0 };
        let Some(g) = adf_generate_adf(Some(p), &mut a) else { return "null".into() };
        let mut h: u64 = 0xcbf29ce484222325;
        for v in &g.cells_and_dists {
            h ^= *v as u32 as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let mut s = format!("{} {} {:x}", g.num_cells, g.cells_and_dists.len(), h);
        for f in [g.fu_to_adf_scale, g.glyph_origin_x, g.glyph_origin_y, g.glyph_min_x, g.glyph_min_y, g.glyph_max_x, g.glyph_max_y] {
            s += &format!(" {}", fb(f));
        }
        let mut h2: u64 = 0xcbf29ce484222325;
        for y in 0..50 {
            for x in 0..50 {
                let pt = [x as f32 / 49.0, y as f32 / 49.0, 0.0];
                h2 ^= adf_reconstruct_dist_abs(&g, &pt).to_bits() as u64;
                h2 = h2.wrapping_mul(0x100000001b3);
            }
        }
        s + &format!(" {:x}", h2)
    }

    fn zone_str(z: &ZoneRecordOut) -> String {
        let mut s = format!("{} {}", z.num_zone_data, z.zone_data.len());
        for l in &z.zone_data {
            s += &format!(" {l}");
        }
        s + &format!(" {}", z.zone_mask)
    }

    #[test]
    fn replay_java_dump() {
        use std::io::BufRead;
        let Ok(file) = std::fs::File::open(DUMP) else { return };
        let (mut fonts, mut states_ok, mut paths, mut paths_ok, mut zones, mut zones_ok, mut adfs, mut adfs_ok) = (0, 0, 0, 0, 0, 0, 0, 0);
        let mut bad = Vec::new();
        let mut lines = std::io::BufReader::new(file).lines().map(|l| l.unwrap()).peekable();
        let mut cur = String::new();
        let mut state: Option<AlgnZoneState> = None;
        let mut glyph: Option<(u16, Vec<ShapeRecordIn>)> = None;
        let mut init: HashMap<u16, Vec<ShapeRecordIn>> = HashMap::new();
        while let Some(l) = lines.next() {
            let l = l.as_str();
            let toks: Vec<&str> = l.split(' ').collect();
            match toks[0] {
                "F" => {
                    cur = l[2..].to_string();
                    state = None;
                    init.clear();
                }
                "S" if toks[1] != "none" => {
                    fonts += 1;
                    let want: Vec<String> = toks[1..5].iter().map(|s| s.to_string()).collect();
                    let mut pending: Vec<(u16, Vec<ShapeRecordIn>)> = Vec::new();
                    while let Some(n) = lines.peek() {
                        if !n.starts_with("I ") && !n.starts_with("P ") {
                            break;
                        }
                        let n = lines.next().unwrap();
                        let n = n.as_str();
                        let t: Vec<&str> = n.split(' ').collect();
                        if t[0] == "I" {
                            let c: u16 = t[1].parse().unwrap();
                            pending.push((c, parse_recs(&t[2..])));
                        } else {
                            let (c, r) = pending.last().unwrap();
                            paths += 1;
                            if path_str(&get_font_outline(r, *c)) == n[2..] {
                                paths_ok += 1;
                            } else if bad.len() < 20 {
                                bad.push(format!("{cur}: init path {c}"));
                            }
                        }
                    }
                    for (c, r) in pending {
                        init.insert(c, r);
                    }
                    let mut order = Vec::new();
                    let st = initialize_align_zone_state(&mut |c| {
                        order.push(c);
                        get_font_outline(&init[&c], c)
                    })
                    .unwrap();
                    assert_eq!(order, vec![90, 122, 76, 108]);
                    let got = [fb(st.left_x_align_pt), fb(st.left_cap_align_pt), fb(st.x_height), fb(st.cap_height)];
                    if got.iter().zip(&want).all(|(a, b)| a == b) {
                        states_ok += 1;
                    } else if bad.len() < 20 {
                        bad.push(format!("{cur}: state {got:?} want {want:?}"));
                    }
                    state = Some(st);
                }
                "G" => {
                    let c: u16 = toks[1].parse().unwrap();
                    let recs = parse_recs(&toks[2..]);
                    if c != 32 {
                        let p = lines.next().unwrap();
                        paths += 1;
                        if path_str(&get_font_outline(&recs, c)) == p[2..] {
                            paths_ok += 1;
                        } else if bad.len() < 20 {
                            bad.push(format!("{cur}: path {c}"));
                        }
                        let h = lines.next().unwrap();
                        adfs += 1;
                        if adf_hash(&get_font_outline(&recs, c)) == h[2..] {
                            adfs_ok += 1;
                        } else if bad.len() < 20 {
                            bad.push(format!("{cur}: adf {c}"));
                        }
                    }
                    glyph = Some((c, recs));
                }
                "Z" => {
                    let (c, recs) = glyph.take().unwrap();
                    let st = state.as_mut().unwrap();
                    zones += 1;
                    let path = if c == 32 { None } else { Some(get_font_outline(&recs, c)) };
                    let z = create_zone_record(st, c, path).unwrap();
                    if zone_str(&z) == l[2..] {
                        zones_ok += 1;
                    } else if bad.len() < 20 {
                        bad.push(format!("{cur}: zone {c} got {} want {}", zone_str(&z), &l[2..]));
                    }
                }
                _ => {}
            }
        }
        eprintln!("fonts {states_ok}/{fonts} paths {paths_ok}/{paths} adf {adfs_ok}/{adfs} zones {zones_ok}/{zones}");
        for b in &bad {
            eprintln!("{b}");
        }
        assert!(bad.is_empty());
    }

    #[test]
    fn replay_java_units() {
        use crate::asc::flex2::fonts::adobe::flashtype::adf_algn_zones::set_glyph_specific_algn_zone_data;
        use crate::asc::flex2::fonts::adobe::flashtype::adf_math_utils::{adf_get_real_cubic_roots, adf_pack_f32, java_acos, java_cos, java_pow_third};
        let Ok(text) = std::fs::read_to_string(DUMP.replace("dump.txt", "units.txt")) else { return };
        let hx = |s: &str| u64::from_str_radix(s, 16).unwrap();
        let mut special = HashMap::new();
        let (mut packs, mut roots, mut roots_ok, mut m_pow, mut m_acos, mut m_cos, mut m_n) = (0, 0, 0, 0, 0, 0, 0);
        for l in text.lines() {
            let t: Vec<&str> = l.split(' ').collect();
            match t[0] {
                "C" => {
                    special.insert(t[1].parse::<u16>().unwrap(), (t[2] == "1", t[3].parse::<i32>().unwrap(), t[4].parse::<i32>().unwrap()));
                }
                "K" => {
                    let r = adf_pack_f32(f32::from_bits(hx(t[1]) as u32));
                    assert_eq!((r.packed_value, r.too_small as i32, r.too_large as i32), (t[2].parse().unwrap(), t[3].parse().unwrap(), t[4].parse().unwrap()), "{l}");
                    packs += 1;
                }
                "R" => {
                    let d: Vec<f64> = t[1..5].iter().map(|s| f64::from_bits(hx(s))).collect();
                    let rv = adf_get_real_cubic_roots(&d);
                    let mut s = format!("{}", rv.num);
                    for j in 0..rv.num as usize {
                        s += &format!(" {:x}", rv.roots.as_ref().unwrap()[j].to_bits());
                    }
                    roots += 1;
                    if s == t[5..].join(" ") {
                        roots_ok += 1;
                    }
                }
                "M" => {
                    let v: Vec<f64> = t[1..].iter().map(|s| f64::from_bits(hx(s))).collect();
                    m_n += 1;
                    m_pow += (java_pow_third(v[0]) != v[1]) as i32;
                    m_acos += (java_acos(v[2]) != v[3]) as i32;
                    m_cos += (java_cos(v[4]) != v[5]) as i32;
                }
                "Q" => {
                    assert_eq!(format!("{} {}", t[1], t[2]), "bfefefd5bfe447f6 3fe6a09e6678b051");
                }
                _ => {}
            }
        }
        for c in 0..=u16::MAX {
            let (mut b, mut i1, mut i2) = (false, 0, 0);
            set_glyph_specific_algn_zone_data(c, &mut b, &mut i1, &mut i2);
            assert_eq!((b, i1, i2), special.get(&c).copied().unwrap_or((true, 0, 0)), "char {c}");
        }
        eprintln!("specials {} packs {packs} cubic roots {roots_ok}/{roots} libm diffs of {m_n}: pow {m_pow} acos {m_acos} cos {m_cos}", special.len());
    }
}
