//! Port of JDK 17 `sun.awt.Win32FontManager`: the Windows overrides of `SunFontManager`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::asc::flex2::fonts::java2d::fontpath;
use crate::asc::flex2::fonts::java2d::sun::font::font2d::{Font2D, TTF_RANK};
use crate::asc::flex2::fonts::java2d::sun::font::sun_font_manager::{t1_filter, tt_filter, FamilyDescription, SunFontManager, FONTFORMAT_TRUETYPE, FONTFORMAT_TYPE1, JRE_FONT_DIR_NAME, PATH_SEPARATOR, SEPARATOR};
use crate::asc::flex2::fonts::java2d::sun::font::true_type_font::TrueTypeFont;
use crate::asc::util::jhash::JHashMap;

/// The `eudcFont` static: the system EUDC font, opened with the Java rasteriser and no file pool.
pub fn eudc_font() -> Option<Rc<Font2D>> {
    let eudc_file = fontpath::get_eudc_font_file()?;
    TrueTypeFont::new(&eudc_file, 0, true).ok().map(|t| Font2D::new_true_type(t, TTF_RANK))
}

/// `registerFontFile(fontFileName, nativeNames, fontRank, defer)`, which takes a base file name and looks for it in
/// the JRE font directory, then on the platform font path.
pub fn register_font_file(fm: &SunFontManager, font_file_name: &str, font_rank: i32, defer: bool) {
    if fm.registered_font_files.borrow().contains(font_file_name) {
        return;
    }
    fm.registered_font_files.borrow_mut().insert(font_file_name.to_string());
    let font_format = if tt_filter(font_file_name) {
        FONTFORMAT_TRUETYPE
    } else if t1_filter(font_file_name) {
        FONTFORMAT_TYPE1
    } else {
        return;
    };
    let font_path = fm.get_platform_font_path();
    let tmp_font_path = format!("{}{}{}", JRE_FONT_DIR_NAME, PATH_SEPARATOR, font_path);
    let mut found = false;
    for new_path in tmp_font_path.split(PATH_SEPARATOR).filter(|s| !s.is_empty()) {
        let is_jre_font = new_path == JRE_FONT_DIR_NAME;
        let the_file = format!("{}{}{}", new_path, SEPARATOR, font_file_name);
        if std::fs::File::open(&the_file).is_ok() {
            found = true;
            let path = std::path::absolute(&the_file).map(|p| p.to_string_lossy().to_string()).unwrap_or(the_file);
            if defer {
                fm.register_deferred_font(font_file_name, &path, font_format, is_jre_font, font_rank);
            } else {
                fm.register_font_file_full(&path, font_format, is_jre_font, font_rank);
            }
            break;
        }
    }
    if !found {
        fm.add_to_missing_font_file_list(font_file_name);
    }
}

pub fn populate_font_file_name_map(font_to_file_map: &mut JHashMap<String, String>, font_to_family_name_map: &mut JHashMap<String, String>, family_to_font_list_map: &mut JHashMap<String, Vec<String>>) {
    fontpath::populate_font_file_name_map0(font_to_file_map, font_to_family_name_map, family_to_font_list_map);
}

pub fn get_font_path(no_type1_fonts: bool) -> String {
    fontpath::get_font_path(no_type1_fonts)
}

/// `getDefaultPlatformFont`: Arial, in the first font directory that has it.
pub fn get_default_platform_font(fm: &SunFontManager) -> (String, String) {
    let mut dir = "c:\\windows\\fonts".to_string();
    let dirs = fm.get_platform_font_dirs();
    if dirs.len() > 1 {
        if let Some(d) = dirs.iter().find(|d| std::path::Path::new(&format!("{}{}arial.ttf", d, SEPARATOR)).exists()) {
            dir = d.clone();
        }
    } else if let Some(d) = dirs.first() {
        dir = d.clone();
    }
    ("Arial".to_string(), format!("{}{}arial.ttf", dir, SEPARATOR))
}

pub fn populate_hardcoded_file_name_map() -> HashMap<String, FamilyDescription> {
    let mut m = HashMap::new();
    let s = |x: &str| Some(x.to_string());
    m.insert(
        "segoe".to_string(),
        FamilyDescription {
            family_name: "Segoe UI".into(),
            plain_full_name: s("Segoe UI"),
            plain_file_name: s("segoeui.ttf"),
            bold_full_name: s("Segoe UI Bold"),
            bold_file_name: s("segoeuib.ttf"),
            italic_full_name: s("Segoe UI Italic"),
            italic_file_name: s("segoeuii.ttf"),
            bold_italic_full_name: s("Segoe UI Bold Italic"),
            bold_italic_file_name: s("segoeuiz.ttf"),
            failed: false,
        },
    );
    m.insert(
        "tahoma".to_string(),
        FamilyDescription { family_name: "Tahoma".into(), plain_full_name: s("Tahoma"), plain_file_name: s("tahoma.ttf"), bold_full_name: s("Tahoma Bold"), bold_file_name: s("tahomabd.ttf"), ..Default::default() },
    );
    m.insert(
        "verdana".to_string(),
        FamilyDescription {
            family_name: "Verdana".into(),
            plain_full_name: s("Verdana"),
            plain_file_name: s("verdana.TTF"),
            bold_full_name: s("Verdana Bold"),
            bold_file_name: s("verdanab.TTF"),
            italic_full_name: s("Verdana Italic"),
            italic_file_name: s("verdanai.TTF"),
            bold_italic_full_name: s("Verdana Bold Italic"),
            bold_italic_file_name: s("verdanaz.TTF"),
            failed: false,
        },
    );
    m.insert(
        "arial".to_string(),
        FamilyDescription {
            family_name: "Arial".into(),
            plain_full_name: s("Arial"),
            plain_file_name: s("ARIAL.TTF"),
            bold_full_name: s("Arial Bold"),
            bold_file_name: s("ARIALBD.TTF"),
            italic_full_name: s("Arial Italic"),
            italic_file_name: s("ARIALI.TTF"),
            bold_italic_full_name: s("Arial Bold Italic"),
            bold_italic_file_name: s("ARIALBI.TTF"),
            failed: false,
        },
    );
    m.insert("symbol".to_string(), FamilyDescription { family_name: "Symbol".into(), plain_full_name: s("Symbol"), plain_file_name: s("Symbol.TTF"), ..Default::default() });
    m.insert("wingdings".to_string(), FamilyDescription { family_name: "WingDings".into(), plain_full_name: s("WingDings"), plain_file_name: s("WINGDING.TTF"), ..Default::default() });
    m
}
