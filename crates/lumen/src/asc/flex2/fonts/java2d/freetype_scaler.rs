//! Port of JDK 17.0.19 `libfontmanager/freetypeScaler.c` (TrueType paths) with the Java side of `sun.font.FreetypeFontScaler`, including `FileFont`'s fallback to `NullFontScaler` once the native scaler is invalidated.

use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::freetype::ftcalc::{ft_matrix_multiply, ft_mul_fix};
use crate::asc::flex2::fonts::java2d::freetype::ftobjs::*;
use crate::asc::flex2::fonts::java2d::freetype::ftoutln::{ft_outline_decompose, ft_outline_embolden, ft_outline_translate};

pub const TEXT_AA_OFF: i32 = 1;
pub const TEXT_AA_ON: i32 = 2;
pub const TEXT_AA_LCD_HRGB: i32 = 4;
pub const TEXT_AA_LCD_HBGR: i32 = 5;
pub const TEXT_AA_LCD_VRGB: i32 = 6;
pub const TEXT_AA_LCD_VBGR: i32 = 7;
pub const TEXT_FM_OFF: i32 = 1;
pub const TEXT_FM_ON: i32 = 2;

pub const INVISIBLE_GLYPHS: i32 = 0xfffe;

pub const SEG_MOVETO: u8 = 0;
pub const SEG_LINETO: u8 = 1;
pub const SEG_QUADTO: u8 = 2;
pub const SEG_CUBICTO: u8 = 3;
pub const SEG_CLOSE: u8 = 4;

const WIND_NON_ZERO: i32 = 0;
const WIND_EVEN_ODD: i32 = 1;
/// `PathIterator.WIND_NON_ZERO`, the rule of `new GeneralPath()`.
pub const JAVA_WIND_NON_ZERO: i32 = 1;

const FT_MATRIX_ONE: FtFixed = 0x10000;
const FT_MATRIX_OBLIQUE_XY: FtFixed = 0x0366A;
const BOLD_DIVISOR: FtFixed = 32;

/// `FTScalerContext`; `is_null` marks `NullFontScaler.getNullScalerContext()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScalerContext {
    pub transform: FtMatrix,
    pub use_sbits: bool,
    pub aa_type: i32,
    pub fm_type: i32,
    pub do_bold: bool,
    pub do_italize: bool,
    pub render_flags: i32,
    pub path_type: i32,
    pub ptsz: i32,
    pub is_null: bool,
}

impl ScalerContext {
    pub fn null() -> ScalerContext {
        ScalerContext { is_null: true, ..ScalerContext::default() }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StrikeMetrics {
    pub ascent_x: f32,
    pub ascent_y: f32,
    pub descent_x: f32,
    pub descent_y: f32,
    pub baseline_x: f32,
    pub baseline_y: f32,
    pub leading_x: f32,
    pub leading_y: f32,
    pub max_advance_x: f32,
    pub max_advance_y: f32,
}

/// A `java.awt.geom.GeneralPath` as the scaler builds it; `winding_rule` is what `getWindingRule()` returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneralPathData {
    pub winding_rule: i32,
    pub types: Vec<u8>,
    pub coords: Vec<f32>,
}

impl GeneralPathData {
    pub fn empty() -> GeneralPathData {
        GeneralPathData { winding_rule: JAVA_WIND_NON_ZERO, types: Vec::new(), coords: Vec::new() }
    }
}

/// `FTScalerInfo` behind a `FreetypeFontScaler`; `face` is None once the scaler is invalidated.
pub struct FreetypeFontScaler {
    face: Option<FtFace>,
}

#[inline]
fn float_to_ft_fixed(f: f32) -> FtFixed {
    msvc_f2i((f * 65536.0f32) as f64)
}

/// MSVC x64 `(int)` of a floating value: truncation, and 0x80000000 when out of range or NaN.
#[inline]
fn msvc_f2i(d: f64) -> i32 {
    if d.is_nan() || d >= 2147483648.0 || d <= -2147483649.0 {
        i32::MIN
    } else {
        d as i32
    }
}

#[inline]
fn ft_fixed_to_float(x: FtFixed) -> f32 {
    x as f32 / 65536.0f32
}

#[inline]
fn ft26dot6_to_float(x: FtPos) -> f32 {
    x as f32 / 64.0f32
}

fn euclidian_distance(a: f64, b: f64) -> f64 {
    let a = if a < 0.0 { -a } else { a };
    let b = if b < 0.0 { -b } else { b };
    if a == 0.0 {
        return b;
    }
    if b == 0.0 {
        return a;
    }
    (a * a + b * b).sqrt()
}

#[inline]
fn too_large(a: f64, b: f64) -> bool {
    msvc_f2i(a / b).wrapping_abs() > 32766
}

fn glyph_slot_embolden(slot: &mut FtGlyphSlot, units_per_em: u16, y_scale: FtFixed, transform: FtMatrix) {
    if slot.format != FT_GLYPH_FORMAT_OUTLINE || slot.metrics.width == 0 || slot.metrics.height == 0 {
        return;
    }
    let extra = ft_mul_fix(units_per_em as i32, y_scale) / BOLD_DIVISOR;
    let _ = ft_outline_embolden(&mut slot.outline, extra);
    slot.metrics.width = slot.metrics.width.wrapping_add(extra);
    slot.metrics.height = slot.metrics.height.wrapping_add(extra);
    if slot.linear_hori_advance == 0 {
        return;
    }
    if slot.advance.x != 0 {
        slot.advance.x = slot.advance.x.wrapping_add(ft_mul_fix(extra, transform.xx));
    }
    if slot.advance.y != 0 {
        slot.advance.y = slot.advance.y.wrapping_add(ft_mul_fix(extra, transform.yx));
    }
    slot.linear_hori_advance = slot.linear_hori_advance.wrapping_add(extra << 10);
    slot.metrics.hori_advance = slot.metrics.hori_advance.wrapping_add(extra);
    slot.metrics.vert_advance = slot.metrics.vert_advance.wrapping_add(extra);
    slot.metrics.hori_bearing_y = slot.metrics.hori_bearing_y.wrapping_add(extra);
}

fn setup_transform(context: &ScalerContext) -> FtMatrix {
    let transform = &context.transform;
    if context.do_italize {
        let mut target = FtMatrix { xx: FT_MATRIX_ONE, xy: FT_MATRIX_OBLIQUE_XY, yx: 0, yy: FT_MATRIX_ONE };
        ft_matrix_multiply(transform, &mut target);
        target
    } else {
        *transform
    }
}

fn setup_ft_context(face: &mut FtFace, context: &ScalerContext) -> FtError {
    let matrix = setup_transform(context);
    ft_set_transform(face, Some(&matrix), None);
    let mut err_code = ft_set_char_size(face, 0, context.ptsz, 72, 72);
    if err_code == 0 {
        err_code = ft_activate_size(face);
    }
    err_code
}

/// `GPData` with the outline callbacks of `addToGP`.
struct GpData {
    wr: i32,
    types: Vec<u8>,
    coords: Vec<f32>,
}

impl GpData {
    fn add_coords(&mut self, p: &FtVector) {
        self.coords.push(ft26dot6_to_float(p.x));
        self.coords.push(-ft26dot6_to_float(p.y));
    }
}

impl FtOutlineFuncs for GpData {
    fn move_to(&mut self, to: &FtVector) -> i32 {
        if !self.coords.is_empty() {
            self.types.push(SEG_CLOSE);
        }
        self.add_coords(to);
        self.types.push(SEG_MOVETO);
        0
    }

    fn line_to(&mut self, to: &FtVector) -> i32 {
        self.add_coords(to);
        self.types.push(SEG_LINETO);
        0
    }

    fn conic_to(&mut self, control: &FtVector, to: &FtVector) -> i32 {
        self.add_coords(control);
        self.add_coords(to);
        self.types.push(SEG_QUADTO);
        0
    }

    fn cubic_to(&mut self, control1: &FtVector, control2: &FtVector, to: &FtVector) -> i32 {
        self.add_coords(control1);
        self.add_coords(control2);
        self.add_coords(to);
        self.types.push(SEG_CUBICTO);
        0
    }
}

fn add_to_gp(gpdata: &mut GpData, outline: &FtOutline) {
    let _ = ft_outline_decompose(outline, gpdata);
    if !gpdata.coords.is_empty() {
        gpdata.types.push(SEG_CLOSE);
    }
    if outline.flags & FT_OUTLINE_EVEN_ODD_FILL != 0 {
        gpdata.wr = WIND_EVEN_ODD;
    }
}

impl FreetypeFontScaler {
    /// `initNativeScaler` for a TrueType font (the file is read through the JDK's stream callback).
    pub fn new(file: Rc<Vec<u8>>, index_in_collection: i32) -> Option<FreetypeFontScaler> {
        match ft_open_face(file, index_in_collection) {
            Ok(face) => Some(FreetypeFontScaler { face: Some(face) }),
            Err(_) => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.face.is_some()
    }

    pub fn face(&self) -> Option<&FtFace> {
        self.face.as_ref()
    }

    /// `invalidateJavaScaler`: later calls behave like `NullFontScaler`.
    fn invalidate(&mut self) {
        self.face = None;
    }

    /// `createScalerContext`; None stands for the 0 the native returns for too large transforms.
    pub fn create_scaler_context(&self, matrix: [f64; 4], aa: i32, fm: i32, boldness: f32, italic: f32) -> Option<ScalerContext> {
        if self.face.is_none() {
            return Some(ScalerContext::null());
        }
        let mut fm = fm;
        let dmat = matrix;
        let mut ptsz = euclidian_distance(dmat[2], dmat[3]);
        if ptsz < 1.0 {
            ptsz = 1.0;
        }
        if ptsz > 16384.0 {
            ptsz = 16384.0;
            fm = TEXT_FM_ON;
        }
        let mut context = ScalerContext::default();
        context.ptsz = msvc_f2i(ptsz * 64.0);
        if too_large(dmat[0], ptsz) || too_large(dmat[1], ptsz) || too_large(dmat[2], ptsz) || too_large(dmat[3], ptsz) {
            return None;
        }
        context.transform.xx = float_to_ft_fixed((dmat[0] / ptsz) as f32);
        context.transform.yx = float_to_ft_fixed((dmat[1] / ptsz) as f32).wrapping_neg();
        context.transform.xy = float_to_ft_fixed((dmat[2] / ptsz) as f32).wrapping_neg();
        context.transform.yy = float_to_ft_fixed((dmat[3] / ptsz) as f32);
        context.aa_type = aa;
        context.fm_type = fm;
        context.do_bold = boldness as f64 != 1.0;
        context.do_italize = italic != 0.0;
        let t = &context.transform;
        if aa != TEXT_AA_ON && fm != TEXT_FM_ON && !context.do_bold && !context.do_italize && t.yx == 0 && t.xy == 0 && t.xx > 0 && t.yy > 0 && t.xx == t.yy {
            context.use_sbits = true;
        }
        Some(context)
    }

    /// `getFontMetricsNative`.
    pub fn get_font_metrics(&mut self, context: &ScalerContext) -> StrikeMetrics {
        let Some(face) = self.face.as_mut() else {
            return StrikeMetrics::default();
        };
        if context.is_null {
            return StrikeMetrics::default();
        }
        if setup_ft_context(face, context) != 0 {
            self.invalidate();
            return StrikeMetrics::default();
        }
        let m = face.size.root_metrics;
        let mul_fix_float_shift6 = |a: i32, b: i32| -> f32 { ((a as f32 * b as f32) as f64 / 65536.0 / 64.0) as f32 };
        let ax: f32 = 0.0;
        let ay = -mul_fix_float_shift6(face.ascender as i32, m.y_scale);
        let dx: f32 = 0.0;
        let dy = -mul_fix_float_shift6(face.descender as i32, m.y_scale);
        let (bx, by): (f32, f32) = (0.0, 0.0);
        let lx: f32 = 0.0;
        let ly = mul_fix_float_shift6(face.height as i32, m.y_scale) + ay - dy;
        let oblique = if context.do_italize { m.height.wrapping_mul(FT_MATRIX_OBLIQUE_XY) / FT_MATRIX_ONE } else { 0 };
        let bold = if context.do_bold { ft_mul_fix(face.units_per_em as i32, m.y_scale) / BOLD_DIVISOR } else { 0 };
        let mx = ft26dot6_to_float(m.max_advance.wrapping_add(oblique).wrapping_add(bold));
        let my: f32 = 0.0;
        let t = &context.transform;
        let cx = |x: f32, y: f32| ft_fixed_to_float(t.xx) * x - ft_fixed_to_float(t.xy) * y;
        let cy = |x: f32, y: f32| -ft_fixed_to_float(t.yx) * x + ft_fixed_to_float(t.yy) * y;
        StrikeMetrics {
            ascent_x: cx(ax, ay),
            ascent_y: cy(ax, ay),
            descent_x: cx(dx, dy),
            descent_y: cy(dx, dy),
            baseline_x: bx,
            baseline_y: by,
            leading_x: cx(lx, ly),
            leading_y: cy(lx, ly),
            max_advance_x: cx(mx, my),
            max_advance_y: cy(mx, my),
        }
    }

    /// `getGlyphImageNativeInternal(renderImage = false)`: the glyph's `(advanceX, advanceY)`.
    fn get_glyph_image_advance(&mut self, context: &ScalerContext, glyph_code: i32) -> (f32, f32) {
        let Some(face) = self.face.as_mut() else {
            return (0.0, 0.0);
        };
        if context.is_null {
            return (0.0, 0.0);
        }
        if setup_ft_context(face, context) != 0 {
            self.invalidate();
            return (0.0, 0.0);
        }
        let mut render_flags = FT_LOAD_DEFAULT;
        if context.aa_type == TEXT_AA_ON && context.fm_type == TEXT_FM_ON {
            render_flags |= FT_LOAD_NO_HINTING;
        }
        if !context.use_sbits {
            render_flags |= FT_LOAD_NO_BITMAP;
        }
        let target = if context.aa_type == TEXT_AA_OFF {
            FT_LOAD_TARGET_MONO
        } else if context.aa_type == TEXT_AA_ON {
            FT_LOAD_TARGET_NORMAL
        } else if context.aa_type == TEXT_AA_LCD_HRGB || context.aa_type == TEXT_AA_LCD_HBGR {
            FT_LOAD_TARGET_LCD
        } else {
            FT_LOAD_TARGET_LCD_V
        };
        render_flags |= target;
        if ft_load_glyph(face, glyph_code as u32, render_flags) != 0 {
            return (0.0, 0.0);
        }
        if context.do_bold {
            let (upem, y_scale) = (face.units_per_em, face.size.root_metrics.y_scale);
            glyph_slot_embolden(&mut face.glyph, upem, y_scale, context.transform);
        }
        let g = &face.glyph;
        if context.fm_type == TEXT_FM_ON {
            let advh = ft_fixed_to_float(g.linear_hori_advance);
            (advh * ft_fixed_to_float(context.transform.xx), -(advh * ft_fixed_to_float(context.transform.yx)))
        } else if g.advance.y == 0 {
            (ft26dot6_to_float(g.advance.x), 0.0)
        } else if g.advance.x == 0 {
            (0.0, ft26dot6_to_float(g.advance.y.wrapping_neg()))
        } else {
            (ft26dot6_to_float(g.advance.x), ft26dot6_to_float(g.advance.y.wrapping_neg()))
        }
    }

    /// `getGlyphAdvanceNative`.
    pub fn get_glyph_advance(&mut self, context: &ScalerContext, glyph_code: i32) -> f32 {
        self.get_glyph_image_advance(context, glyph_code).0
    }

    /// `getGlyphMetricsNative`: `(x, y)` of the advance.
    pub fn get_glyph_metrics(&mut self, context: &ScalerContext, glyph_code: i32) -> (f32, f32) {
        self.get_glyph_image_advance(context, glyph_code)
    }

    /// `getFTOutline`: the loaded (unhinted unless tricky) outline in 26.6, or None.
    fn get_ft_outline(&mut self, context: &ScalerContext, glyph_code: i32, xpos: f32, ypos: f32) -> Option<&FtOutline> {
        if glyph_code >= INVISIBLE_GLYPHS || context.is_null {
            return None;
        }
        let face = self.face.as_mut()?;
        if setup_ft_context(face, context) != 0 {
            return None;
        }
        let render_flags = FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP;
        if ft_load_glyph(face, glyph_code as u32, render_flags) != 0 {
            return None;
        }
        if context.do_bold {
            let (upem, y_scale) = (face.units_per_em, face.size.root_metrics.y_scale);
            glyph_slot_embolden(&mut face.glyph, upem, y_scale, context.transform);
        }
        let to_26dot6 = |x: f32| msvc_f2u((x * 64.0f32) as f64) as i32;
        ft_outline_translate(&mut face.glyph.outline, to_26dot6(xpos), to_26dot6(-ypos));
        Some(&face.glyph.outline)
    }

    /// `getGlyphOutlineNative`.
    pub fn get_glyph_outline(&mut self, context: &ScalerContext, glyph_code: i32, xpos: f32, ypos: f32) -> GeneralPathData {
        let Some(outline) = self.get_ft_outline(context, glyph_code, xpos, ypos) else {
            return GeneralPathData::empty();
        };
        if outline.n_points == 0 {
            return GeneralPathData::empty();
        }
        let mut gpdata = GpData { wr: WIND_NON_ZERO, types: Vec::new(), coords: Vec::new() };
        add_to_gp(&mut gpdata, outline);
        GeneralPathData { winding_rule: gpdata.wr, types: gpdata.types, coords: gpdata.coords }
    }

    /// `getGlyphVectorOutlineNative`; as in the JDK, only the last glyph with points survives.
    pub fn get_glyph_vector_outline(&mut self, context: &ScalerContext, glyphs: &[i32], xpos: f32, ypos: f32) -> GeneralPathData {
        if glyphs.is_empty() {
            return GeneralPathData::empty();
        }
        let mut gpdata = GpData { wr: WIND_NON_ZERO, types: Vec::new(), coords: Vec::new() };
        for &g in glyphs {
            if g >= INVISIBLE_GLYPHS {
                continue;
            }
            let Some(outline) = self.get_ft_outline(context, g, xpos, ypos) else {
                continue;
            };
            if outline.n_points == 0 {
                continue;
            }
            gpdata = GpData { wr: WIND_NON_ZERO, types: Vec::new(), coords: Vec::new() };
            add_to_gp(&mut gpdata, outline);
        }
        if !gpdata.coords.is_empty() {
            return GeneralPathData { winding_rule: gpdata.wr, types: gpdata.types, coords: gpdata.coords };
        }
        GeneralPathData::empty()
    }

    /// `getNumGlyphsNative` (1 for an invalidated scaler).
    pub fn get_num_glyphs(&self) -> i32 {
        match &self.face {
            Some(f) => f.num_glyphs,
            None => 1,
        }
    }

    /// `getMissingGlyphCodeNative`.
    pub fn get_missing_glyph_code(&self) -> i32 {
        0
    }

    /// `getUnitsPerEMNative` (2048 for an invalidated scaler).
    pub fn get_units_per_em(&self) -> i64 {
        match &self.face {
            Some(f) => f.units_per_em as i64,
            None => 2048,
        }
    }
}

/// MSVC x64 `(unsigned int)` of a floating value (via a 64-bit truncation).
#[inline]
fn msvc_f2u(d: f64) -> u32 {
    if d.is_nan() || d >= 9223372036854775808.0 || d < -9223372036854775808.0 {
        0
    } else {
        d as i64 as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::BufRead;

    fn crc32_table() -> [u32; 256] {
        let mut t = [0u32; 256];
        for i in 0..256u32 {
            let mut c = i;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            t[i as usize] = c;
        }
        t
    }

    fn path_crc(t: &[u32; 256], gp: &GeneralPathData) -> (u32, usize) {
        let mut crc = !0u32;
        let upd = |b: u8, crc: &mut u32| *crc = t[((*crc ^ b as u32) & 0xFF) as usize] ^ (*crc >> 8);
        upd(gp.winding_rule as u8, &mut crc);
        let mut ci = 0;
        for &ty in &gp.types {
            let k = match ty {
                SEG_CLOSE => 0,
                SEG_QUADTO => 4,
                SEG_CUBICTO => 6,
                _ => 2,
            };
            upd(ty, &mut crc);
            for _ in 0..k {
                for b in gp.coords[ci].to_bits().to_be_bytes() {
                    upd(b, &mut crc);
                }
                ci += 1;
            }
        }
        (!crc, gp.types.len())
    }

    fn hx(f: f32) -> String {
        format!("{:x}", f.to_bits())
    }

    #[test]
    fn replay_ftdump() {
        if std::env::var_os("LUMEN_FONT_REPLAY").is_none() {
            return;
        }
        let path = std::env::var("FT_DUMP").unwrap_or_else(|_| r"S:\Desktop\Projects\Rust-FFDec\target\fonts_ws\java2d_ft\ftdump.txt".to_string());
        let Ok(file) = std::fs::File::open(&path) else {
            return;
        };
        let filter = std::env::var("FT_FILTER").ok().map(|s| s.to_lowercase());
        let verbose = std::env::var("FT_VERBOSE").is_ok();
        let reverse = std::env::var("FT_REVERSE").is_ok();
        let sz: f64 = std::env::var("FT_SIZE").ok().and_then(|v| v.parse().ok()).unwrap_or(240.0);
        let t = crc32_table();
        let rd = std::io::BufReader::with_capacity(1 << 20, file);
        let mut lines = rd.lines();
        let mut faces = 0usize;
        let mut glyphs = 0usize;
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        let mut bad_faces: Vec<String> = Vec::new();
        let mut scaler: Option<FreetypeFontScaler> = None;
        let mut skip = false;
        let mut face_desc = String::new();
        let mut face_bad: BTreeMap<&str, usize> = BTreeMap::new();
        let mut first_bad = String::new();
        let mut cur_st = 0;
        let mut contexts: Option<(ScalerContext, ScalerContext)> = None;
        let mut autohint = false;
        let mut autohint_mismatches = 0usize;
        let mut cff_faces = 0usize;
        let flush = |desc: &str, fb: &mut BTreeMap<&'static str, usize>, first: &mut String, bad: &mut Vec<String>| {
            if !fb.is_empty() {
                bad.push(format!("{} {:?} first: {}", desc, fb, first));
            }
            fb.clear();
            first.clear();
        };
        while let Some(Ok(line)) = lines.next() {
            let mut parts = line.splitn(2, '\t');
            let tag = parts.next().unwrap();
            let rest = parts.next().unwrap_or("");
            match tag {
                "F" => {
                    flush(&face_desc, &mut face_bad, &mut first_bad, &mut bad_faces);
                    let f: Vec<&str> = rest.split('\t').collect();
                    face_desc = format!("{}#{} {}", f[0], f[1], f[2]);
                    skip = filter.as_ref().map_or(false, |flt| !face_desc.to_lowercase().contains(flt.as_str()));
                    if skip {
                        continue;
                    }
                    faces += 1;
                    autohint = false;
                    let data = std::fs::read(f[0]).expect("font file");
                    if data.starts_with(b"OTTO") {
                        eprintln!("CFF (not ported) {}", face_desc);
                        cff_faces += 1;
                        skip = true;
                        continue;
                    }
                    scaler = FreetypeFontScaler::new(Rc::new(data), f[1].parse().unwrap());
                    if let Some(fc) = scaler.as_ref().and_then(|s| s.face()) {
                        let tt = &fc.tt;
                        if fc.is_scalable() && !fc.is_tricky() && tt.num_locations != 0 && tt.font_program.is_empty() && tt.cvt_program.len() <= 7 {
                            eprintln!("AUTOHINT {}", face_desc);
                            autohint = true;
                        }
                        if fc.is_tricky() {
                            eprintln!("TRICKY {}", face_desc);
                        }
                    }
                    let n: i32 = f[4].parse().unwrap();
                    let upem: i64 = f[5].parse().unwrap();
                    let sc = scaler.as_ref();
                    if sc.map(|s| s.get_num_glyphs()) != Some(n) || sc.map(|s| s.get_units_per_em()) != Some(upem) {
                        *kinds.entry("face").or_default() += 1;
                        *face_bad.entry("face").or_default() += 1;
                        first_bad = format!("numGlyphs/upem {:?}/{:?} vs {}/{}", sc.map(|s| s.get_num_glyphs()), sc.map(|s| s.get_units_per_em()), n, upem);
                    }
                }
                "S" if !skip => {
                    let v: Vec<&str> = rest.split(' ').collect();
                    cur_st = v[0].parse::<i32>().unwrap();
                    let Some(sc) = scaler.as_mut() else { continue };
                    let bold = if cur_st & 1 != 0 { 1.33f32 } else { 1.0 };
                    let ital = if cur_st & 2 != 0 { 0.7f32 } else { 0.0 };
                    let ch = sc.create_scaler_context([sz, 0.0, 0.0, sz], 2, 1, bold, ital).unwrap();
                    let cl = sc.create_scaler_context([sz, 0.0, 0.0, sz], 2, 2, bold, ital).unwrap();
                    let m = sc.get_font_metrics(&ch);
                    let got = [m.ascent_x, m.ascent_y, m.descent_x, m.descent_y, m.baseline_x, m.baseline_y, m.leading_x, m.leading_y, m.max_advance_x, m.max_advance_y].map(hx).join(" ");
                    if got != v[1..].join(" ") {
                        *kinds.entry("metrics").or_default() += 1;
                        *face_bad.entry("metrics").or_default() += 1;
                        if first_bad.is_empty() {
                            first_bad = format!("st{} metrics {} vs {}", cur_st, got, v[1..].join(" "));
                        }
                    }
                    contexts = Some((ch, cl));
                }
                _ => {}
            }
            if !skip && !line.starts_with("F	") && !line.starts_with("S	") && !line.starts_with("D	") && !line.is_empty() {
                let Some(sc) = scaler.as_mut() else { continue };
                let (ch, cl) = contexts.unwrap();
                let entries: Vec<&str> = line.split(';').filter(|e| !e.is_empty()).collect();
                let order: Vec<usize> = if reverse { (0..entries.len()).rev().collect() } else { (0..entries.len()).collect() };
                for g in order {
                    let e: Vec<&str> = entries[g].split(' ').collect();
                    glyphs += 1;
                    let g = g as i32;
                    let ha = hx(sc.get_glyph_advance(&ch, g));
                    let la = hx(sc.get_glyph_advance(&cl, g));
                    let gp = sc.get_glyph_outline(&ch, g, 0.0, 0.0);
                    let (crc, segs) = path_crc(&t, &gp);
                    let mut note = |k: &'static str, detail: String| {
                        *kinds.entry(k).or_default() += 1;
                        *face_bad.entry(k).or_default() += 1;
                        if first_bad.is_empty() || verbose {
                            if verbose {
                                eprintln!("{} {}", face_desc, detail);
                            }
                            if first_bad.is_empty() {
                                first_bad = detail;
                            }
                        }
                    };
                    if ha != e[0] && autohint {
                        autohint_mismatches += 1;
                    } else if ha != e[0] {
                        note("hinted", format!("st{} g{} hinted {} vs {}", cur_st, g, ha, e[0]));
                    }
                    if la != e[1] {
                        note("linear", format!("st{} g{} linear {} vs {}", cur_st, g, la, e[1]));
                    }
                    if format!("{:x}", crc) != e[2] || segs.to_string() != e[3] {
                        note("outline", format!("st{} g{} outline {:x}/{} vs {}/{}", cur_st, g, crc, segs, e[2], e[3]));
                    }
                }
            }
        }
        flush(&face_desc, &mut face_bad, &mut first_bad, &mut bad_faces);
        eprintln!("faces {} (CFF skipped {}) glyph entries {} mismatches {:?}, hinted advances of auto-hinted faces (not ported) {}", faces, cff_faces, glyphs, kinds, autohint_mismatches);
        for b in &bad_faces {
            eprintln!("BAD {}", b);
        }
        assert!(bad_faces.is_empty());
    }
}
