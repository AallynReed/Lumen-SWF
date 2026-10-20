//! A small TrueType writer: quadratic outlines, one glyph per character, no hinting.

use crate::shape::Point;

pub struct Glyph {
    pub unicode: u32,
    pub advance: u16,
    /// Contours in font units, y pointing up.
    pub contours: Vec<Vec<Point>>,
}

pub struct Font {
    pub family: String,
    pub copyright: String,
    pub bold: bool,
    pub italic: bool,
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub glyphs: Vec<Glyph>,
}

pub const UNITS_PER_EM: u16 = 1024;

#[derive(Default)]
struct Buf(Vec<u8>);

impl Buf {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn pad4(&mut self) {
        while self.0.len() % 4 != 0 {
            self.0.push(0);
        }
    }
}

#[derive(Clone, Copy, Default)]
struct BBox {
    x0: i16,
    y0: i16,
    x1: i16,
    y1: i16,
    empty: bool,
}

fn bbox(contours: &[Vec<Point>]) -> BBox {
    let mut it = contours.iter().flatten();
    let Some(first) = it.next() else { return BBox { empty: true, ..Default::default() } };
    let mut b = BBox { x0: first.x as i16, y0: first.y as i16, x1: first.x as i16, y1: first.y as i16, empty: false };
    for p in it {
        b.x0 = b.x0.min(p.x as i16);
        b.y0 = b.y0.min(p.y as i16);
        b.x1 = b.x1.max(p.x as i16);
        b.y1 = b.y1.max(p.y as i16);
    }
    b
}

fn area(c: &[Point]) -> i64 {
    let n = c.len();
    (0..n).map(|i| {
        let (a, b) = (c[i], c[(i + 1) % n]);
        a.x as i64 * b.y as i64 - b.x as i64 * a.y as i64
    }).sum()
}

fn contains(c: &[Point], x: i32, y: i32) -> bool {
    let n = c.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (c[i], c[j]);
        if (a.y > y) != (b.y > y) {
            let t = (b.x - a.x) as f64 * (y - a.y) as f64 / (b.y - a.y) as f64 + a.x as f64;
            if (x as f64) < t {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// SWF glyphs fill even-odd; TrueType fills non-zero. Orient each contour by its nesting
/// depth (outer clockwise, holes counter-clockwise) so both rules agree.
pub fn normalize(mut contours: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    for c in contours.iter_mut() {
        while c.len() > 1 && c.first().map(|p| (p.x, p.y)) == c.last().map(|p| (p.x, p.y)) && c.last().unwrap().on {
            c.pop();
        }
    }
    contours.retain(|c| c.len() >= 2);
    let depth: Vec<usize> = (0..contours.len())
        .map(|i| {
            let p = contours[i][0];
            (0..contours.len()).filter(|&j| j != i && contains(&contours[j], p.x, p.y)).count()
        })
        .collect();
    for (c, d) in contours.iter_mut().zip(depth) {
        let clockwise = area(c) < 0;
        if clockwise != (d % 2 == 0) {
            c.reverse();
            // Keep an on-curve point first.
            if let Some(i) = c.iter().position(|p| p.on) {
                c.rotate_left(i);
            }
        }
    }
    contours
}

fn glyph_data(contours: &[Vec<Point>], b: BBox) -> Vec<u8> {
    let mut g = Buf::default();
    if b.empty {
        return Vec::new();
    }
    g.i16(contours.len() as i16);
    g.i16(b.x0);
    g.i16(b.y0);
    g.i16(b.x1);
    g.i16(b.y1);
    let mut end = 0u16;
    for c in contours {
        end += c.len() as u16;
        g.u16(end - 1);
    }
    g.u16(0);
    let pts: Vec<Point> = contours.iter().flatten().copied().collect();
    for p in &pts {
        g.u8(p.on as u8);
    }
    let (mut px, mut py) = (0i32, 0i32);
    for p in &pts {
        g.i16((p.x - px) as i16);
        px = p.x;
    }
    for p in &pts {
        g.i16((p.y - py) as i16);
        py = p.y;
    }
    g.0
}

fn name_table(f: &Font) -> Vec<u8> {
    let sub = match (f.bold, f.italic) {
        (true, true) => "Bold Italic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        _ => "Regular",
    };
    let ps: String = format!("{}-{}", f.family, sub.replace(' ', "")).chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let full = if sub == "Regular" { f.family.clone() } else { format!("{} {}", f.family, sub) };
    let records = [
        (0u16, f.copyright.clone()),
        (1, f.family.clone()),
        (2, sub.to_string()),
        (3, format!("Rust-FFDec: {full}")),
        (4, full),
        (5, "Version 1.0".to_string()),
        (6, ps),
    ];
    let mut strings = Vec::new();
    let mut t = Buf::default();
    t.u16(0);
    t.u16(records.len() as u16);
    t.u16(6 + 12 * records.len() as u16);
    for (id, s) in &records {
        let enc: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        t.u16(3);
        t.u16(1);
        t.u16(0x409);
        t.u16(*id);
        t.u16(enc.len() as u16);
        t.u16(strings.len() as u16);
        strings.extend_from_slice(&enc);
    }
    t.0.extend_from_slice(&strings);
    t.0
}

fn cmap_table(map: &[(u32, u16)]) -> Vec<u8> {
    let bmp: Vec<&(u32, u16)> = map.iter().filter(|(c, _)| *c <= 0xfffe).collect();
    let mut f4 = Buf::default();
    let seg = bmp.len() as u16 + 1;
    let search = 2 * (1u16 << (15 - seg.leading_zeros()));
    f4.u16(4);
    f4.u16(16 + 8 * seg);
    f4.u16(0);
    f4.u16(seg * 2);
    f4.u16(search);
    f4.u16((search / 2).trailing_zeros() as u16);
    f4.u16(seg * 2 - search);
    for (c, _) in &bmp {
        f4.u16(*c as u16);
    }
    f4.u16(0xffff);
    f4.u16(0);
    for (c, _) in &bmp {
        f4.u16(*c as u16);
    }
    f4.u16(0xffff);
    for (c, g) in &bmp {
        f4.u16(g.wrapping_sub(*c as u16));
    }
    f4.u16(1);
    for _ in 0..seg {
        f4.u16(0);
    }
    let mut f12 = Buf::default();
    f12.u16(12);
    f12.u16(0);
    f12.u32(16 + 12 * map.len() as u32);
    f12.u32(0);
    f12.u32(map.len() as u32);
    for (c, g) in map {
        f12.u32(*c);
        f12.u32(*c);
        f12.u32(*g as u32);
    }
    let mut t = Buf::default();
    t.u16(0);
    t.u16(2);
    t.u16(3);
    t.u16(1);
    t.u32(4 + 16);
    t.u16(3);
    t.u16(10);
    t.u32(4 + 16 + f4.0.len() as u32);
    t.0.extend_from_slice(&f4.0);
    t.0.extend_from_slice(&f12.0);
    t.0
}

fn checksum(d: &[u8]) -> u32 {
    d.chunks(4).fold(0u32, |s, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(w))
    })
}

pub fn build(font: &Font) -> Vec<u8> {
    let mut glyphs: Vec<(Vec<Vec<Point>>, u16)> = vec![(Vec::new(), UNITS_PER_EM / 2)];
    let mut map = Vec::new();
    for g in &font.glyphs {
        map.push((g.unicode, glyphs.len() as u16));
        glyphs.push((normalize(g.contours.clone()), g.advance));
    }
    map.sort_by_key(|m| m.0);
    map.dedup_by_key(|m| m.0);

    let mut glyf = Buf::default();
    let mut loca = Buf::default();
    let mut hmtx = Buf::default();
    let mut total = BBox { empty: true, ..Default::default() };
    let (mut max_points, mut max_contours, mut adv_max) = (0u16, 0u16, 0u16);
    let (mut min_lsb, mut min_rsb, mut max_extent) = (i16::MAX, i16::MAX, i16::MIN);
    for (contours, adv) in &glyphs {
        let b = bbox(contours);
        loca.u32(glyf.0.len() as u32);
        glyf.0.extend_from_slice(&glyph_data(contours, b));
        glyf.pad4();
        hmtx.u16(*adv);
        hmtx.i16(if b.empty { 0 } else { b.x0 });
        adv_max = adv_max.max(*adv);
        if !b.empty {
            max_points = max_points.max(contours.iter().map(Vec::len).sum::<usize>() as u16);
            max_contours = max_contours.max(contours.len() as u16);
            min_lsb = min_lsb.min(b.x0);
            min_rsb = min_rsb.min(*adv as i16 - b.x1);
            max_extent = max_extent.max(b.x1);
            if total.empty {
                total = b;
            } else {
                total.x0 = total.x0.min(b.x0);
                total.y0 = total.y0.min(b.y0);
                total.x1 = total.x1.max(b.x1);
                total.y1 = total.y1.max(b.y1);
            }
        }
    }
    loca.u32(glyf.0.len() as u32);
    if total.empty {
        min_lsb = 0;
        min_rsb = 0;
        max_extent = 0;
    }
    let n = glyphs.len() as u16;
    let mac_style = (font.bold as u16) | ((font.italic as u16) << 1);

    let mut head = Buf::default();
    head.u32(0x00010000);
    head.u32(0x00010000);
    head.u32(0);
    head.u32(0x5F0F3CF5);
    head.u16(0x000B);
    head.u16(UNITS_PER_EM);
    // Seconds since 1904-01-01.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + 2_082_844_800;
    head.i64(now);
    head.i64(now);
    head.i16(total.x0);
    head.i16(total.y0);
    head.i16(total.x1);
    head.i16(total.y1);
    head.u16(mac_style);
    head.u16(8);
    head.i16(2);
    head.i16(1);
    head.i16(0);

    let mut hhea = Buf::default();
    hhea.u32(0x00010000);
    hhea.i16(font.ascent);
    hhea.i16(-font.descent);
    hhea.i16(font.line_gap);
    hhea.u16(adv_max);
    hhea.i16(min_lsb);
    hhea.i16(min_rsb);
    hhea.i16(max_extent);
    hhea.i16(1);
    hhea.i16(0);
    hhea.i16(0);
    for _ in 0..4 {
        hhea.i16(0);
    }
    hhea.i16(0);
    hhea.u16(n);

    let mut maxp = Buf::default();
    maxp.u32(0x00010000);
    maxp.u16(n);
    maxp.u16(max_points);
    maxp.u16(max_contours);
    maxp.u16(0);
    maxp.u16(0);
    maxp.u16(2);
    for _ in 0..8 {
        maxp.u16(0);
    }

    let first = map.first().map_or(0, |m| m.0.min(0xffff)) as u16;
    let last = map.last().map_or(0, |m| m.0.min(0xffff)) as u16;
    let avg = if glyphs.len() > 1 { (glyphs[1..].iter().map(|g| g.1 as u32).sum::<u32>() / (glyphs.len() as u32 - 1)) as i16 } else { 0 };
    let mut os2 = Buf::default();
    os2.u16(3);
    os2.i16(avg);
    os2.u16(if font.bold { 700 } else { 400 });
    os2.u16(5);
    os2.u16(0);
    for v in [650, 600, 0, 75, 650, 600, 0, 350, 50, 260] {
        os2.i16(v);
    }
    os2.i16(0);
    os2.0.extend_from_slice(&[0; 10]);
    for _ in 0..4 {
        os2.u32(0);
    }
    os2.0.extend_from_slice(b"RFFD");
    let fs_selection = if font.bold || font.italic { (font.italic as u16) | ((font.bold as u16) << 5) } else { 0x40 };
    os2.u16(fs_selection);
    os2.u16(first);
    os2.u16(last);
    os2.i16(font.ascent);
    os2.i16(-font.descent);
    os2.i16(font.line_gap);
    os2.u16(font.ascent.max(total.y1).max(0) as u16);
    os2.u16(font.descent.max(-total.y0).max(0) as u16);
    os2.u32(1);
    os2.u32(0);
    os2.i16(0);
    os2.i16(0);
    os2.u16(0);
    os2.u16(32);
    os2.u16(0);

    let mut post = Buf::default();
    post.u32(0x00030000);
    post.u32(if font.italic { (-12i32 << 16) as u32 } else { 0 });
    post.i16(-100);
    post.i16(50);
    for _ in 0..5 {
        post.u32(0);
    }

    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"OS/2", os2.0),
        (*b"cmap", cmap_table(&map)),
        (*b"glyf", glyf.0),
        (*b"head", head.0),
        (*b"hhea", hhea.0),
        (*b"hmtx", hmtx.0),
        (*b"loca", loca.0),
        (*b"maxp", maxp.0),
        (*b"name", name_table(font)),
        (*b"post", post.0),
    ];
    tables.sort_by_key(|t| t.0);

    let count = tables.len() as u16;
    let search = 16 * (1u16 << (15 - count.leading_zeros()));
    let mut out = Buf::default();
    out.u32(0x00010000);
    out.u16(count);
    out.u16(search);
    out.u16((search / 16).trailing_zeros() as u16);
    out.u16(count * 16 - search);
    let mut offset = 12 + 16 * tables.len() as u32;
    let mut head_offset = 0;
    for (tag, data) in &tables {
        out.0.extend_from_slice(tag);
        out.u32(checksum(data));
        out.u32(offset);
        out.u32(data.len() as u32);
        if tag == b"head" {
            head_offset = offset as usize;
        }
        offset += (data.len() as u32 + 3) & !3;
    }
    for (_, data) in &tables {
        out.0.extend_from_slice(data);
        out.pad4();
    }
    let adj = 0xB1B0AFBAu32.wrapping_sub(checksum(&out.0));
    out.0[head_offset + 8..head_offset + 12].copy_from_slice(&adj.to_be_bytes());
    out.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Point> {
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].iter().map(|&(x, y)| Point { x, y, on: true }).collect()
    }

    #[test]
    fn hole_is_counter_wound() {
        let c = normalize(vec![sq(0, 0, 100, 100), sq(25, 25, 75, 75)]);
        assert!(area(&c[0]) < 0);
        assert!(area(&c[1]) > 0);
    }

    #[test]
    fn builds_valid_directory() {
        let f = Font {
            family: "Test".into(),
            copyright: String::new(),
            bold: false,
            italic: false,
            ascent: 800,
            descent: 200,
            line_gap: 0,
            glyphs: vec![Glyph { unicode: 'A' as u32, advance: 600, contours: vec![sq(0, 0, 500, 700)] }],
        };
        let d = build(&f);
        assert_eq!(&d[..4], &[0, 1, 0, 0]);
        assert_eq!(u16::from_be_bytes([d[4], d[5]]), 10);
        assert_eq!(checksum(&d), 0xB1B0AFBA);
    }
}
