//! Port of JDK 17 `windows/native/libfontmanager/fontpath.c` (`Win32FontManager.getFontPath`,
//! `populateFontFileNameMap0`) and `Win32FontManager.getEUDCFontFile`, calling the same GDI and registry APIs.
//! Elsewhere they report no fonts.

use crate::asc::util::jhash::JHashMap;

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    pub const LF_FACESIZE: usize = 32;
    pub const DEFAULT_CHARSET: u8 = 1;
    pub const TRUETYPE_FONTTYPE: u32 = 4;
    pub const DEVICE_FONTTYPE: u32 = 2;
    pub const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
    pub const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as i32 as isize;
    pub const KEY_READ: u32 = 0x20019;
    pub const REG_SZ: u32 = 1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct LogFontW {
        pub lf_height: i32,
        pub lf_width: i32,
        pub lf_escapement: i32,
        pub lf_orientation: i32,
        pub lf_weight: i32,
        pub lf_italic: u8,
        pub lf_underline: u8,
        pub lf_strike_out: u8,
        pub lf_char_set: u8,
        pub lf_out_precision: u8,
        pub lf_clip_precision: u8,
        pub lf_quality: u8,
        pub lf_pitch_and_family: u8,
        pub lf_face_name: [u16; LF_FACESIZE],
    }

    #[repr(C)]
    pub struct EnumLogFontExW {
        pub elf_log_font: LogFontW,
        pub elf_full_name: [u16; 64],
        pub elf_style: [u16; 32],
        pub elf_script: [u16; 32],
    }

    pub type FontEnumProcW = unsafe extern "system" fn(*const EnumLogFontExW, *const c_void, u32, isize) -> i32;

    #[link(name = "gdi32", kind = "raw-dylib")]
    extern "system" {
        pub fn EnumFontFamiliesExW(hdc: isize, lf: *const LogFontW, proc_: FontEnumProcW, lparam: isize, flags: u32) -> i32;
    }

    #[link(name = "user32", kind = "raw-dylib")]
    extern "system" {
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    }

    #[link(name = "advapi32", kind = "raw-dylib")]
    extern "system" {
        pub fn RegOpenKeyExW(key: isize, sub_key: *const u16, options: u32, sam: u32, result: *mut isize) -> i32;
        pub fn RegQueryInfoKeyW(key: isize, class: *mut u16, class_len: *mut u32, reserved: *mut u32, sub_keys: *mut u32, max_sub_key_len: *mut u32, max_class_len: *mut u32, values: *mut u32, max_value_name_len: *mut u32, max_value_len: *mut u32, security: *mut u32, last_write: *mut c_void) -> i32;
        pub fn RegEnumValueW(key: isize, index: u32, value_name: *mut u16, value_name_len: *mut u32, reserved: *mut u32, typ: *mut u32, data: *mut u8, data_len: *mut u32) -> i32;
        pub fn RegQueryValueExW(key: isize, value_name: *const u16, reserved: *mut u32, typ: *mut u32, data: *mut u8, data_len: *mut u32) -> i32;
        pub fn RegCloseKey(key: isize) -> i32;
    }

    #[link(name = "kernel32", kind = "raw-dylib")]
    extern "system" {
        pub fn GetSystemDirectoryW(buf: *mut u16, size: u32) -> u32;
        pub fn GetWindowsDirectoryW(buf: *mut u16, size: u32) -> u32;
        pub fn GetSystemDefaultLangID() -> u16;
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn wcslen(s: &[u16]) -> usize {
        s.iter().position(|&c| c == 0).unwrap_or(s.len())
    }

    pub fn from_wide(s: &[u16]) -> String {
        String::from_utf16_lossy(&s[..wcslen(s)])
    }
}

const BSIZE: usize = 512;

/// `getFontPath`: the Fonts directories next to the system and Windows directories, `;`-separated when they differ.
pub fn get_font_path(_no_type1: bool) -> String {
    #[cfg(windows)]
    unsafe {
        let mut buf = [0u16; BSIZE];
        let n = win::GetSystemDirectoryW(buf.as_mut_ptr(), BSIZE as u32) as usize;
        let mut sysdir = String::from_utf16_lossy(&buf[..n.min(BSIZE)]);
        if let Some(end) = sysdir.rfind('\\') {
            sysdir.truncate(end);
            sysdir.push_str("\\Fonts");
        }
        let n = win::GetWindowsDirectoryW(buf.as_mut_ptr(), BSIZE as u32) as usize;
        let mut windir = String::from_utf16_lossy(&buf[..n.min(BSIZE)]);
        if windir.len() > BSIZE - 7 {
            windir.clear();
        } else {
            windir.push_str("\\Fonts");
        }
        let mut fontpath = sysdir.clone();
        if !sysdir.eq_ignore_ascii_case(&windir) {
            fontpath.push(';');
            fontpath.push_str(&windir);
        }
        fontpath
    }
    #[cfg(not(windows))]
    String::new()
}

#[cfg(windows)]
struct GdiFontMapInfo<'a> {
    family: String,
    font_to_family_map: &'a mut JHashMap<String, String>,
    family_to_font_list_map: &'a mut JHashMap<String, Vec<String>>,
    list_key: String,
    screen_dc: isize,
}

#[cfg(windows)]
struct CheckFamilyInfo {
    family: Vec<u16>,
    is_different: bool,
}

#[cfg(windows)]
unsafe extern "system" fn check_font_family_proc_w(lpelfe: *const win::EnumLogFontExW, _ntm: *const std::ffi::c_void, _font_type: u32, lparam: isize) -> i32 {
    let info = &mut *(lparam as *mut CheckFamilyInfo);
    let face = &(*lpelfe).elf_log_font.lf_face_name;
    info.is_different = face[..win::wcslen(face)] != info.family[..win::wcslen(&info.family)];
    0
}

#[cfg(windows)]
unsafe fn different_family(screen_dc: isize, family: &[u16], full_name: &[u16]) -> bool {
    if win::wcslen(full_name) >= win::LF_FACESIZE {
        return false;
    }
    let mut info = CheckFamilyInfo { family: family.to_vec(), is_different: false };
    let mut lfw: win::LogFontW = std::mem::zeroed();
    let n = win::wcslen(full_name).min(win::LF_FACESIZE - 1);
    lfw.lf_face_name[..n].copy_from_slice(&full_name[..n]);
    lfw.lf_char_set = win::DEFAULT_CHARSET;
    win::EnumFontFamiliesExW(screen_dc, &lfw, check_font_family_proc_w, &mut info as *mut CheckFamilyInfo as isize, 0);
    info.is_different
}

#[cfg(windows)]
unsafe extern "system" fn enum_font_faces_in_family_proc_w(lpelfe: *const win::EnumLogFontExW, _ntm: *const std::ffi::c_void, font_type: u32, lparam: isize) -> i32 {
    let fmi = &mut *(lparam as *mut GdiFontMapInfo);
    if font_type != win::TRUETYPE_FONTTYPE && font_type != win::DEVICE_FONTTYPE {
        return 1;
    }
    let e = &*lpelfe;
    if different_family(fmi.screen_dc, &e.elf_log_font.lf_face_name, &e.elf_full_name) {
        return 1;
    }
    let fullname = win::from_wide(&e.elf_full_name);
    if let Some(list) = fmi.family_to_font_list_map.get_mut(&fmi.list_key) {
        list.push(fullname.clone());
    }
    let fullname_lc = fullname.to_lowercase();
    fmi.font_to_family_map.insert(fullname_lc, fmi.family.clone());
    1
}

#[cfg(windows)]
unsafe extern "system" fn enum_family_names_w(lpelfe: *const win::EnumLogFontExW, _ntm: *const std::ffi::c_void, font_type: u32, lparam: isize) -> i32 {
    let fmi = &mut *(lparam as *mut GdiFontMapInfo);
    if font_type != win::TRUETYPE_FONTTYPE && font_type != win::DEVICE_FONTTYPE {
        return 1;
    }
    let e = &*lpelfe;
    if e.elf_log_font.lf_face_name[0] == '@' as u16 {
        return 1;
    }
    fmi.family = win::from_wide(&e.elf_log_font.lf_face_name);
    let family_lc = fmi.family.to_lowercase();
    if fmi.family_to_font_list_map.contains_key(&family_lc) {
        return 1;
    }
    fmi.family_to_font_list_map.insert(family_lc.clone(), Vec::with_capacity(4));
    fmi.list_key = family_lc;
    let mut lfw: win::LogFontW = std::mem::zeroed();
    let n = win::wcslen(&e.elf_log_font.lf_face_name).min(win::LF_FACESIZE - 1);
    lfw.lf_face_name[..n].copy_from_slice(&e.elf_log_font.lf_face_name[..n]);
    lfw.lf_char_set = e.elf_log_font.lf_char_set;
    win::EnumFontFamiliesExW(fmi.screen_dc, &lfw, enum_font_faces_in_family_proc_w, lparam, 0);
    1
}

/// `RegistryToBaseTTNameW`: strips a " (TrueType)" or " (OpenType)" suffix, false when there is none.
fn registry_to_base_tt_name(name: &mut String) -> bool {
    let units: Vec<u16> = name.encode_utf16().collect();
    let len = units.len();
    let ttslen = " (TrueType)".len();
    if len == 0 || units[len - 1] != ')' as u16 || len <= ttslen {
        return false;
    }
    let suffix = String::from_utf16_lossy(&units[len - ttslen..]);
    if suffix == " (TrueType)" || suffix == " (OpenType)" {
        *name = String::from_utf16_lossy(&units[..len - ttslen]);
        return true;
    }
    false
}

fn register_font_w(font_to_file_map: &mut JHashMap<String, String>, name: &str, data: &str) {
    let file_str = data.to_string();
    let last = data.chars().last();
    if matches!(last, Some('C') | Some('c')) && name.contains(" & ") {
        for part in name.split(" & ").collect::<Vec<_>>().into_iter().rev() {
            font_to_file_map.insert(part.to_lowercase(), file_str.clone());
        }
    } else {
        font_to_file_map.insert(name.to_lowercase(), file_str);
    }
}

#[cfg(windows)]
unsafe fn populate_font_file_name_from_registry_key(reg_key: isize, font_to_file_map: &mut JHashMap<String, String>) {
    const MAX_BUFFER: u32 = 260 + 1;
    let mut hkey_fonts: isize = 0;
    let key = win::wide("Software\\Microsoft\\Windows NT\\CurrentVersion\\Fonts");
    if win::RegOpenKeyExW(reg_key, key.as_ptr(), 0, win::KEY_READ, &mut hkey_fonts) != 0 {
        return;
    }
    let (mut num_values, mut max_name, mut max_data) = (0u32, 0u32, 0u32);
    let ret = win::RegQueryInfoKeyW(hkey_fonts, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &mut num_values, &mut max_name, &mut max_data, std::ptr::null_mut(), std::ptr::null_mut());
    if ret != 0 || max_name >= MAX_BUFFER || max_data >= MAX_BUFFER {
        win::RegCloseKey(hkey_fonts);
        return;
    }
    let mut wname = vec![0u16; MAX_BUFFER as usize];
    let mut data = vec![0u8; MAX_BUFFER as usize + 1];
    for nval in 0..num_values {
        let mut name_size = MAX_BUFFER;
        let mut data_size = MAX_BUFFER;
        let mut typ = 0u32;
        data.iter_mut().for_each(|b| *b = 0);
        let ret = win::RegEnumValueW(hkey_fonts, nval, wname.as_mut_ptr(), &mut name_size, std::ptr::null_mut(), &mut typ, data.as_mut_ptr(), &mut data_size);
        if ret != 0 {
            break;
        }
        if typ != win::REG_SZ {
            continue;
        }
        let mut name = String::from_utf16_lossy(&wname[..name_size as usize]);
        let dw: Vec<u16> = data[..data_size as usize].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        let data_s = String::from_utf16_lossy(&dw[..win::wcslen(&dw)]);
        if !registry_to_base_tt_name(&mut name) {
            let ok = match data_s.rfind('.') {
                None => false,
                Some(d) => {
                    let ext = data_s[d..].to_lowercase();
                    ext == ".ttf" || ext == ".otf"
                }
            };
            if !ok {
                continue;
            }
        }
        register_font_w(font_to_file_map, &name, &data_s);
    }
    win::RegCloseKey(hkey_fonts);
}

/// `populateFontFileNameMap0(fontToFileMap, fontToFamilyMap, familyToFontListMap, Locale.ENGLISH)`.
pub fn populate_font_file_name_map0(font_to_file_map: &mut JHashMap<String, String>, font_to_family_map: &mut JHashMap<String, String>, family_to_font_list_map: &mut JHashMap<String, Vec<String>>) {
    #[cfg(windows)]
    unsafe {
        let screen_dc = win::GetDC(0);
        if screen_dc == 0 {
            return;
        }
        let mut fmi = GdiFontMapInfo { family: String::new(), font_to_family_map, family_to_font_list_map, list_key: String::new(), screen_dc };
        let mut lfw: win::LogFontW = std::mem::zeroed();
        lfw.lf_char_set = win::DEFAULT_CHARSET;
        win::EnumFontFamiliesExW(screen_dc, &lfw, enum_family_names_w, &mut fmi as *mut GdiFontMapInfo as isize, 0);
        populate_font_file_name_from_registry_key(win::HKEY_CURRENT_USER, font_to_file_map);
        populate_font_file_name_from_registry_key(win::HKEY_LOCAL_MACHINE, font_to_file_map);
        win::ReleaseDC(0, screen_dc);
    }
    #[cfg(not(windows))]
    {
        let _ = (font_to_file_map, font_to_family_map, family_to_font_list_map);
    }
}

/// `getEUDCFontFile`: the `SystemDefaultEUDCFont` of the system language's code page, under `HKCU\EUDC`.
pub fn get_eudc_font_file() -> Option<String> {
    #[cfg(windows)]
    unsafe {
        let key = match win::GetSystemDefaultLangID() {
            0x0411 => "EUDC\\932",
            0x0804 | 0x1004 => "EUDC\\936",
            0x0c04 | 0x0404 | 0x1404 => "EUDC\\950",
            0x0412 => "EUDC\\949",
            0x0409 => "EUDC\\1252",
            _ => return None,
        };
        let mut hkey: isize = 0;
        let k = win::wide(key);
        if win::RegOpenKeyExW(win::HKEY_CURRENT_USER, k.as_ptr(), 0, win::KEY_READ, &mut hkey) != 0 {
            return None;
        }
        let mut buf = vec![0u8; (260 + 1) * 2];
        let mut len = buf.len() as u32;
        let mut typ = 0u32;
        let v = win::wide("SystemDefaultEUDCFont");
        let rc = win::RegQueryValueExW(hkey, v.as_ptr(), std::ptr::null_mut(), &mut typ, buf.as_mut_ptr(), &mut len);
        win::RegCloseKey(hkey);
        if rc != 0 || typ != win::REG_SZ {
            return None;
        }
        let w: Vec<u16> = buf[..len as usize].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
        let path = win::from_wide(&w);
        if path.contains('\\') {
            return Some(path);
        }
        let mut wd = [0u16; BSIZE];
        let n = win::GetWindowsDirectoryW(wd.as_mut_ptr(), BSIZE as u32) as usize;
        Some(format!("{}\\FONTS\\{}", String::from_utf16_lossy(&wd[..n.min(BSIZE)]), path))
    }
    #[cfg(not(windows))]
    None
}
