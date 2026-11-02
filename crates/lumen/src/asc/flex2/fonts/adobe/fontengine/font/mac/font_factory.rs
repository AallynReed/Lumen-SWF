//! Port of afe.jar `com.adobe.fontengine.font.mac.FontFactory`: the resource-font sniffing only (the resource fork
//! parser is not ported).

use std::io::Read;
use std::path::Path;

pub struct FontFactory;

impl FontFactory {
    pub const UNKNOWN: i32 = 0;
    pub const RESOURCE_FORK_FONT: i32 = 1;
    pub const DATA_FORK_FONT: i32 = 2;

    pub fn get_num_bytes_needed_to_identify() -> i32 {
        16
    }

    /// `isResourceFont(bytes, url)` for a file: a `.dfont` data-fork font whose header looks sane; the
    /// `/..namedfork/rsrc` resource fork never opens off macOS.
    pub fn is_resource_font(_b: &[u8], url_file: &str, path: &Path) -> i32 {
        if !url_file.ends_with(".dfont") {
            return 0;
        }
        let Ok(mut f) = std::fs::File::open(path) else { return 0 };
        let mut h = [0u8; 16];
        if f.read_exact(&mut h).is_err() {
            return 0;
        }
        let u = |i: usize| (h[i] as i64) << 24 | (h[i + 1] as i64) << 16 | (h[i + 2] as i64) << 8 | h[i + 3] as i64;
        let (l4, l3, l2, l) = (u(0), u(4), u(8), u(12));
        if l3 > l4 && l3 < l4 + l2 || l4 > l3 && l4 < l3 + l {
            return 0;
        }
        Self::DATA_FORK_FONT
    }
}
