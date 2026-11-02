//! `java.awt.Font` and `GraphicsEnvironment` as JREFontManager calls them, on the JDK 17 Java2D port in `fonts::java2d`.

use crate::asc::flex2::fonts::java2d::java::util::locale::Locale;
use crate::asc::flex2::fonts::java2d::sun::font::font_design_metrics::FontDesignMetrics;
use crate::asc::flex2::fonts::java2d::sun::font::font_strike_desc::{INTVAL_FRACTIONALMETRICS_DEFAULT, INTVAL_FRACTIONALMETRICS_OFF, INTVAL_TEXT_ANTIALIAS_ON};
use crate::asc::flex2::fonts::java2d::sun::font::standard_glyph_vector::StandardGlyphVector;
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::SunFontManager;

pub use crate::asc::flex2::fonts::java2d::java::awt::font::Font;

impl Font {
    /// `Font.createFont(Font.TRUETYPE_FONT, new FileInputStream(path))`.
    pub fn create_font_truetype(path: &std::path::Path) -> Result<Font, String> {
        Font::create_font(&path.to_string_lossy()).map_err(|e| e.0)
    }

    /// `deriveFont(attributes)` with FAMILY, SIZE, POSTURE and WEIGHT put into `getAttributes()`.
    pub fn derive_font_attrs(&self, family: &str, size: f32, oblique: bool, bold: bool) -> Font {
        self.derive_font(family, size, oblique, bold)
    }

    /// `getFontName(Locale.ENGLISH)`.
    pub fn get_font_name_english(&self) -> String {
        self.get_font_name(&Locale::english())
    }

    /// `graphics.getFontMetrics(font)` (antialiased) `getAscent()`/`getDescent()`.
    pub fn font_metrics_ascent_descent(&self) -> (i32, i32) {
        let m = FontDesignMetrics::get_metrics(self, INTVAL_TEXT_ANTIALIAS_ON, INTVAL_FRACTIONALMETRICS_DEFAULT);
        (m.get_ascent(), m.get_descent())
    }

    /// `createGlyphVector(new FontRenderContext(null, true, false), [c])`: the outline's segments and the advance.
    pub fn glyph_outline_and_advance(&self, c: u16) -> (Vec<(i32, [f32; 6])>, f32) {
        let mut gv = StandardGlyphVector::new(self, &[c], INTVAL_TEXT_ANTIALIAS_ON, INTVAL_FRACTIONALMETRICS_OFF);
        let outline = gv.get_glyph_outline(0);
        let advance = gv.get_glyph_metrics_advance(0);
        (outline.segments(), advance)
    }
}

/// `GraphicsEnvironment.getLocalGraphicsEnvironment().getAvailableFontFamilyNames()` (no fonts registered through
/// `registerFont`, so only the installed families).
pub fn get_available_font_family_names() -> Vec<String> {
    SunFontManager::get_instance().get_installed_font_family_names(Some(&Locale::get_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const STYLES: [&str; 4] = ["plain", "bold", "italic", "bolditalic"];
    /// Faces whose FreeType advances need the unported auto-hinter.
    const AUTO_HINTED: [&str; 5] = ["Ink Free", "Sans Serif Collection", "Segoe Fluent Icons", "SimSun-ExtG", "HarmonyOS Sans SC"];

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xffffffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb88320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    fn fb(f: f32) -> String {
        format!("{:x}", f.to_bits())
    }

    #[derive(Default)]
    struct Stats {
        kinds: BTreeMap<String, (usize, usize)>,
        shown: usize,
    }

    impl Stats {
        /// Prints the counts and fails on any mismatch outside the documented gaps (`.gap.*` kinds).
        fn finish(&self) {
            let mut bad = 0;
            for (k, (ok, nok)) in &self.kinds {
                eprintln!("{k:28} match {ok:9} mismatch {nok}");
                if !k.contains(".gap") {
                    bad += nok;
                }
            }
            assert_eq!(bad, 0);
        }

        fn check(&mut self, kind: &str, ok: bool, what: impl FnOnce() -> String) {
            let e = self.kinds.entry(kind.to_string()).or_default();
            if ok {
                e.0 += 1;
            } else {
                e.1 += 1;
                if self.shown < 400 && !kind.contains(".gap") {
                    self.shown += 1;
                    eprintln!("MISMATCH {kind}: {}", what());
                }
            }
        }
    }

    /// The `Dump.face` fields after the key, in its call order (metrics first).
    fn face_fields(font: &Font) -> Vec<String> {
        let (ascent, descent) = font.font_metrics_ascent_descent();
        let f2d = font.get_font2d();
        let (class, plat) = match f2d.plat_name() {
            Some(p) => ("TrueTypeFont".to_string(), p.to_string()),
            None => ("CompositeFont".to_string(), "-".to_string()),
        };
        vec![
            font.get_style().to_string(),
            font.get_name(),
            font.get_family(),
            font.get_font_name_english(),
            font.get_ps_name(),
            fb(font.get_size2d()),
            font.get_num_glyphs().to_string(),
            font.get_missing_glyph_code().to_string(),
            ascent.to_string(),
            descent.to_string(),
            class,
            plat,
            f2d.get_style().to_string(),
            f2d.get_font_name(None),
        ]
    }

    /// `Dump.glyph`: advance bits, segment count, CRC32 of each type byte and its coordinates' big-endian float bits.
    fn glyph_fields(font: &Font, c: u16) -> (String, usize, String, String) {
        let (segs, adv) = font.glyph_outline_and_advance(c);
        let mut bytes = Vec::new();
        let mut sb = String::new();
        for (t, co) in &segs {
            let k = match t {
                0 | 1 => 2,
                2 => 4,
                3 => 6,
                _ => 0,
            };
            bytes.push(*t as u8);
            sb.push_str(&format!(" {t}"));
            for v in &co[..k] {
                bytes.extend_from_slice(&v.to_bits().to_be_bytes());
                sb.push_str(&format!(",{:x}", v.to_bits()));
            }
        }
        let crc = if bytes.is_empty() { 0 } else { crc32(&bytes) };
        (fb(adv), segs.len(), format!("{:x}", crc), sb.trim().to_string())
    }

    /// Replays `Dump.java` (`S:\Claude Work\fonts\j2d\harness`): the family list, then every `Font.decode(fam-style-240)`
    /// and `createFont`/`deriveFont` face field by field, its canDisplay bitmap and its glyphs.
    #[test]
    fn replay_java2d_dump() {
        if std::env::var_os("LUMEN_FONT_REPLAY").is_none() {
            return;
        }
        let path = std::env::var("J2D_DUMP").unwrap_or_else(|_| "S:/Claude Work/fonts/j2d/dump.txt".into());
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let mut st = Stats::default();
        replay(&text, &mut st);
        st.finish();
    }

    /// `Scen.java` runs, each a call sequence in a fresh JVM, replayed each on a fresh thread (fresh font manager).
    #[test]
    fn replay_java2d_scenarios() {
        if std::env::var_os("LUMEN_FONT_REPLAY").is_none() {
            return;
        }
        let dir = std::env::var("J2D_SCEN").unwrap_or_else(|_| "S:/Claude Work/fonts/j2d/scen".into());
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let mut st = Stats::default();
        let mut files: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        files.sort();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let sub = std::thread::spawn(move || {
                let mut st = Stats::default();
                replay(&text, &mut st);
                st
            })
            .join()
            .unwrap();
            for (k, (ok, nok)) in sub.kinds {
                let e = st.kinds.entry(k).or_default();
                e.0 += ok;
                e.1 += nok;
            }
        }
        st.finish();
    }

    fn replay(text: &str, st: &mut Stats) {
        let media = std::env::var("J2D_MEDIA").unwrap_or_else(|_| "S:/Claude Work/fonts/media/".into());
        let lines: Vec<&str> = text.lines().collect();
        let n_fams: usize = lines[0].split('\t').nth(1).unwrap().parse().unwrap();
        let fams: Vec<&str> = lines[1..1 + n_fams].iter().map(|l| &l[4..]).collect();
        let got = get_available_font_family_names();
        st.check("families", got.len() == fams.len(), || format!("count {} vs {}", got.len(), fams.len()));
        for (k, f) in fams.iter().enumerate() {
            st.check("family", got.get(k).map(|g| g == f).unwrap_or(false), || format!("[{k}] {:?} vs {f}", got.get(k)));
        }
        let mut i = 1 + n_fams;
        let mut created: Option<Font> = None;
        let mut cff_metrics = std::collections::HashSet::new();
        let fields = ["style", "name", "family", "fontName", "psName", "size", "numGlyphs", "missing", "ascent", "descent", "f2dClass", "platName", "f2dStyle", "f2dName"];
        while i < lines.len() {
            let p: Vec<&str> = lines[i].split('\t').collect();
            match p[0] {
                "CREATEFAIL" => {
                    let r = Font::create_font(&format!("{media}{}", p[1]));
                    st.check("createfail", r.is_err(), || p[1].to_string());
                    created = None;
                    i += 1;
                }
                "CREATE" => {
                    match Font::create_font(&format!("{media}{}", p[1])) {
                        Ok(f) => {
                            let mine = format!("{}\t{}\t{}\t{}\t{}", f.get_family(), f.get_font_name_english(), f.get_name(), f.get_style(), fb(f.get_size2d()));
                            let want = p[2..].join("\t");
                            st.check("create", mine == want, || format!("{}: {mine} vs {want}", p[1]));
                            created = Some(f);
                        }
                        Err(e) => {
                            st.check("create", false, || format!("{}: {e:?}", p[1]));
                            created = None;
                        }
                    }
                    i += 1;
                }
                "DECODE" | "DERIVE" => {
                    let is_derive = p[0] == "DERIVE";
                    let s: usize = p[2].parse().unwrap();
                    let font = if is_derive {
                        let Some(c) = &created else {
                            while !lines[i].starts_with("END") {
                                i += 1;
                            }
                            i += 1;
                            continue;
                        };
                        let family = c.get_family();
                        c.derive_font_attrs(&family, 240.0, s & 2 != 0, s & 1 != 0)
                    } else {
                        Font::decode(&format!("{}-{}-240", p[1], STYLES[s]))
                    };
                    let key = format!("{} {} {}", p[0], p[1], p[2]);
                    let mut mine = face_fields(&font);
                    let mut want: Vec<String> = p[3..].iter().map(|s| s.to_string()).collect();
                    if is_derive && want[11].contains("+~JF") && mine[11].starts_with(&media) {
                        want[11] = "TEMP".into();
                        mine[11] = "TEMP".into();
                    }
                    let metrics_key = (font.get_name(), font.get_style());
                    let cff = font.get_font2d().ttf().map(|t| t.is_cff()).unwrap_or(false);
                    if cff {
                        cff_metrics.insert(metrics_key.clone());
                    }
                    let cff_cached = cff_metrics.contains(&metrics_key);
                    let gap = if cff {
                        ".gap.cff"
                    } else if AUTO_HINTED.iter().any(|a| want[13].starts_with(a)) {
                        ".gap.autohint"
                    } else {
                        ""
                    };
                    for (k, fname) in fields.iter().enumerate() {
                        let g = if cff_cached && (*fname == "ascent" || *fname == "descent") { ".gap.cff" } else { "" };
                        st.check(&format!("face.{fname}{g}"), mine[k] == want[k], || format!("{key}: {:?} vs {:?}", mine[k], want[k]));
                    }
                    let can = lines[i + 1].strip_prefix("CAN\t").unwrap().as_bytes();
                    let mut ndiff = 0;
                    let mut first = None;
                    for c in 0..0x10000usize {
                        let byte = u8::from_str_radix(std::str::from_utf8(&can[(c >> 3) * 2..(c >> 3) * 2 + 2]).unwrap(), 16).unwrap();
                        if font.can_display(c as u16) != ((byte >> (c & 7)) & 1 != 0) {
                            ndiff += 1;
                            first.get_or_insert(c);
                        }
                    }
                    st.check("canDisplay", ndiff == 0, || format!("{key}: {ndiff} chars differ, first U+{:04x}", first.unwrap_or(0)));
                    i += 2;
                    while !lines[i].starts_with("END") {
                        let g: Vec<&str> = lines[i].split('\t').collect();
                        let c = u16::from_str_radix(g[1], 16).unwrap();
                        let (adv, n, crc, segs) = glyph_fields(&font, c);
                        let agap = if gap.is_empty() { "" } else { gap };
                        st.check(&format!("glyph.advance{agap}"), adv == g[2], || format!("{key} U+{c:04x}: adv {adv} vs {}", g[2]));
                        let ok = n.to_string() == g[3] && crc == g[4] && (g.len() < 6 || segs == g[5]);
                        let ogap = if cff { gap } else { "" };
                        st.check(&format!("glyph.outline{ogap}"), ok, || format!("{key} U+{c:04x}: {n} {crc} vs {} {}", g[3], g[4]));
                        i += 1;
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }
}
