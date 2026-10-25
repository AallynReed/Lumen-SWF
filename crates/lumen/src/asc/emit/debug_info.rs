//! Port of `macromedia/asc/embedding/avmplus/DebugInfo.java`.

pub struct DebugInfo {
    pub debug_linenum: i32,
    pub debug_linenum_dirty: bool,
    pub debug_file: Option<String>,
    pub suppress_debug_method: bool,
    pub debug_file_dirty: bool,
}

impl Default for DebugInfo {
    fn default() -> Self {
        DebugInfo { debug_linenum: -1, debug_linenum_dirty: false, debug_file: None, suppress_debug_method: false, debug_file_dirty: false }
    }
}
