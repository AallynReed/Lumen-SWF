//! Export names (ExportAssets) and AS3 class names (SymbolClass) per character, and the
//! file names derived from them.

use std::collections::HashMap;

use crate::io::Reader;
use crate::tags::code::{EXPORT_ASSETS, SYMBOL_CLASS};
use crate::Swf;

#[derive(Default)]
pub struct Names {
    pub export: HashMap<u16, String>,
    pub classes: HashMap<u16, Vec<String>>,
}

impl Names {
    pub fn new(swf: &Swf) -> Names {
        let mut n = Names::default();
        swf.walk(|_, t| {
            if t.code != EXPORT_ASSETS && t.code != SYMBOL_CLASS {
                return;
            }
            let mut r = Reader::new(&t.body);
            let Ok(count) = r.u16() else { return };
            for _ in 0..count {
                let (Ok(id), Ok(name)) = (r.u16(), r.string()) else { break };
                if t.code == EXPORT_ASSETS {
                    n.export.entry(id).or_insert(name);
                } else {
                    let list = n.classes.entry(id).or_default();
                    if !list.contains(&name) {
                        list.push(name);
                    }
                }
            }
        });
        n
    }

    /// `id`, then `_exportName` and `_ClassName` (when there is exactly one class).
    pub fn export_file_name(&self, id: u16) -> String {
        let mut s = id.to_string();
        if let Some(e) = self.export.get(&id) {
            s.push('_');
            s.push_str(e);
        }
        if let Some([c]) = self.classes.get(&id).map(Vec::as_slice) {
            s.push('_');
            s.push_str(c);
        }
        s
    }
}

/// Percent-encodes characters Windows rejects in file names.
pub fn make_file_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let code = c as u32;
        if code < 32 || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push_str(&format!("%{code:02X}"));
        } else {
            out.push(c);
        }
    }
    if out.ends_with(' ') {
        out.pop();
        out.push_str("%20");
    } else if out.ends_with('.') {
        out.pop();
        out.push_str("%2E");
    }
    const RESERVED: [&str; 23] = [
        "CON", "PRN", "AUX", "CLOCK$", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.iter().any(|r| out == *r || out.strip_prefix(r).is_some_and(|rest| rest.starts_with('.'))) {
        out.insert(0, '_');
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::make_file_name;

    #[test]
    fn file_names() {
        assert_eq!(make_file_name("a/b:c"), "a%2Fb%3Ac");
        assert_eq!(make_file_name("trail."), "trail%2E");
        assert_eq!(make_file_name("CON"), "_CON");
        assert_eq!(make_file_name("NUL.txt"), "_NUL.txt");
        assert_eq!(make_file_name("CONSOLE"), "CONSOLE");
        assert_eq!(make_file_name(""), "unnamed");
    }
}
