//! Port of `flex2.tools.SwcChecksums`: the checksums an incremental compile stores with its cache and compares on
//! the next run.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::compiler_swc_context::CompilerSwcContext;
use crate::asc::flex2::compiler::util::QName;
use crate::asc::util::jhash::JHashMap;

pub struct SwcChecksums {
    /// `checksum`, `cmdChecksum`, `linkChecksum`, `swcChecksum`.
    pub checksums: [i32; 4],
    pub swc_def_signature_checksums: Option<JHashMap<QName, i64>>,
    pub swc_file_checksums: Option<JHashMap<String, i64>>,
    pub archive_file_checksums: JHashMap<String, i64>,
    is_swc_checksum_enabled: bool,
}

impl SwcChecksums {
    pub fn new(swc_context: &CompilerSwcContext, checksum_ts: i32, link_checksum_ts: i32, is_swc_checksum_enabled: bool) -> SwcChecksums {
        SwcChecksums {
            checksums: [0, checksum_ts, link_checksum_ts, swc_context.checksum()],
            swc_def_signature_checksums: Some(JHashMap::new()),
            swc_file_checksums: Some(JHashMap::new()),
            archive_file_checksums: JHashMap::new(),
            is_swc_checksum_enabled,
        }
    }

    /// `saveSignatureChecksums(units)`.
    fn save_signature_checksums(&mut self, units: &[UnitRef]) {
        if !self.is_swc_checksum_enabled {
            self.swc_def_signature_checksums = None;
            return;
        }
        let mut m = JHashMap::new();
        for unit in units {
            let u = unit.borrow();
            let s = u.get_source();
            let s = s.borrow();
            if !s.is_swc_script_owner() || s.is_internal() {
                continue;
            }
            let checksum = u.signature_checksum.unwrap_or_else(|| s.swc_script().map_or(0, |sc| sc.get_last_modified()));
            for q in u.top_level_definitions.iter() {
                m.insert(q.clone(), checksum);
            }
        }
        self.swc_def_signature_checksums = Some(m);
    }

    /// `saveSwcFileChecksums()`: into the map loaded from the cache, if any.
    fn save_swc_file_checksums(&mut self, swc_context: &CompilerSwcContext) {
        if !self.is_swc_checksum_enabled {
            self.swc_file_checksums = None;
            return;
        }
        let m = self.swc_file_checksums.get_or_insert_with(JHashMap::new);
        for (name, t) in swc_context.get_files() {
            m.insert(name, t);
        }
        for (name, t) in swc_context.get_theme_style_sheets() {
            m.insert(name, t);
        }
    }

    /// `saveChecksums(units)`.
    pub fn save_checksums(&mut self, units: &[UnitRef], swc_context: &CompilerSwcContext) {
        self.save_swc_file_checksums(swc_context);
        self.save_signature_checksums(units);
        self.checksums[0] = self.calculate_checksum(swc_context);
    }

    /// `calculateChecksum()`.
    fn calculate_checksum(&self, swc_context: &CompilerSwcContext) -> i32 {
        let mut checksum = self.checksums[1];
        if !self.is_swc_checksum_enabled || self.swc_def_signature_checksums.as_ref().is_none_or(|m| m.is_empty()) {
            checksum = checksum.wrapping_add(swc_context.checksum());
        }
        checksum
    }

    /// `isRecompilationNeeded(loadedChecksums)`.
    pub fn is_recompilation_needed(&mut self, loaded: &[i32; 4], swc_context: &mut CompilerSwcContext) -> bool {
        self.checksums[0] = self.calculate_checksum(swc_context);
        if self.checksums[0] != loaded[0] {
            return true;
        }
        if !self.is_swc_checksum_enabled {
            return false;
        }
        if let Some(m) = &self.swc_def_signature_checksums {
            for (q, data) in m.iter() {
                let mut swc = swc_context.get_checksum(q);
                if swc.is_none() {
                    swc = swc_context.get_source(q.get_namespace(), q.get_local_part()).map(|s| s.borrow().get_last_modified());
                }
                match swc {
                    None => return true,
                    Some(v) if v != *data => return true,
                    _ => {}
                }
            }
        }
        !self.are_swc_file_checksums_equal(swc_context)
    }

    /// `isRelinkNeeded(loadedChecksums)`.
    pub fn is_relink_needed(&self, loaded: &[i32; 4]) -> bool {
        if self.checksums[2] != loaded[2] {
            return true;
        }
        !self.are_archive_file_checksums_equal()
    }

    /// `areSwcFileChecksumsEqual()`: the theme style sheets join the library files first.
    fn are_swc_file_checksums_equal(&self, swc_context: &mut CompilerSwcContext) -> bool {
        let Some(data) = &self.swc_file_checksums else { return false };
        swc_context.add_theme_style_sheets_to_files();
        let files = swc_context.get_files();
        if files.len() < data.len() {
            return false;
        }
        for (name, t) in data.iter() {
            match files.iter().find(|(n, _)| n == name) {
                Some((_, ft)) if ft == t => {}
                _ => return false,
            }
        }
        true
    }

    /// `areArchiveFileChecksumsEqual()`.
    fn are_archive_file_checksums_equal(&self) -> bool {
        if self.swc_file_checksums.is_none() {
            return false;
        }
        for (name, t) in self.archive_file_checksums.iter() {
            let lm = std::fs::metadata(name).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
            if lm != *t {
                return false;
            }
        }
        true
    }
}
