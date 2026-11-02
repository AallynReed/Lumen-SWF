//! Port of `flex2.compiler.util.OrderedProperties` over `java.util.Properties` as JDK 9+ implements it: a
//! `ConcurrentHashMap(8)`, whose iteration order `entrySet()` (what the i18n compiler walks) exposes.

use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::util::jstr;

const WHITESPACE: &str = " \t\n\r\x0c";
const VALTERMINATORS: &str = "\n\r\x0c";

/// `java.util.concurrent.ConcurrentHashMap`'s table: bins in index order, each bin in its `next` order, split on
/// `transfer` the way JDK 17 does it (the `lastRun` tail kept, the nodes before it reversed onto each half).
#[derive(Clone, Debug)]
pub struct ConcurrentHashMap<V> {
    table: Vec<Vec<(i32, String, V)>>,
    size: usize,
    size_ctl: usize,
}

fn spread(h: i32) -> i32 {
    (h ^ ((h as u32) >> 16) as i32) & 0x7fff_ffff
}

fn table_size_for(c: usize) -> usize {
    c.next_power_of_two()
}

impl<V: Clone> ConcurrentHashMap<V> {
    /// `new ConcurrentHashMap<>(initialCapacity)`.
    pub fn new(initial_capacity: usize) -> Self {
        let cap = table_size_for(initial_capacity + (initial_capacity >> 1) + 1);
        ConcurrentHashMap { table: Vec::new(), size: 0, size_ctl: cap }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        if self.table.is_empty() {
            return None;
        }
        let h = spread(jstr::java_hash_code(key));
        let n = self.table.len();
        self.table[(n - 1) & h as usize].iter().find(|(eh, k, _)| *eh == h && k == key).map(|e| &e.2)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn put(&mut self, key: &str, value: V) -> Option<V> {
        if self.table.is_empty() {
            self.table = vec![Vec::new(); self.size_ctl];
            self.size_ctl = self.table.len() - (self.table.len() >> 2);
        }
        let h = spread(jstr::java_hash_code(key));
        let n = self.table.len();
        let bin = &mut self.table[(n - 1) & h as usize];
        if let Some(e) = bin.iter_mut().find(|(eh, k, _)| *eh == h && k == key) {
            return Some(std::mem::replace(&mut e.2, value));
        }
        bin.push((h, key.to_string(), value));
        let bin_count = bin.len() - 1;
        if bin_count >= 8 && n < 64 {
            let c = table_size_for((n << 1) + (n << 1 >> 1) + 1);
            while c > self.size_ctl {
                self.transfer();
            }
        }
        self.size += 1;
        while self.size >= self.size_ctl && self.table.len() < (1 << 30) {
            self.transfer();
        }
        None
    }

    fn transfer(&mut self) {
        let n = self.table.len();
        let mut next: Vec<Vec<(i32, String, V)>> = vec![Vec::new(); n << 1];
        for i in (0..n).rev() {
            let f = std::mem::take(&mut self.table[i]);
            if f.is_empty() {
                continue;
            }
            let bit = |e: &(i32, String, V)| (e.0 as usize) & n;
            let mut run_bit = bit(&f[0]);
            let mut last_run = 0;
            for (j, p) in f.iter().enumerate().skip(1) {
                let b = bit(p);
                if b != run_bit {
                    run_bit = b;
                    last_run = j;
                }
            }
            let mut ln: Vec<(i32, String, V)> = Vec::new();
            let mut hn: Vec<(i32, String, V)> = Vec::new();
            if run_bit == 0 {
                ln.extend(f[last_run..].iter().cloned());
            } else {
                hn.extend(f[last_run..].iter().cloned());
            }
            for p in &f[..last_run] {
                if bit(p) == 0 {
                    ln.insert(0, p.clone());
                } else {
                    hn.insert(0, p.clone());
                }
            }
            next[i] = ln;
            next[i + n] = hn;
        }
        self.table = next;
        self.size_ctl = (n << 1) - (n >> 1);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.table.iter().flat_map(|b| b.iter().map(|(_, k, v)| (k.as_str(), v)))
    }
}

/// `OrderedProperties`, with values of any type (Java stores `Object`s through `put`).
#[derive(Clone, Debug)]
pub struct OrderedProperties<V> {
    pub map: ConcurrentHashMap<V>,
    keys: Vec<String>,
    pub lines: std::collections::HashMap<String, i32>,
}

impl<V: Clone> Default for OrderedProperties<V> {
    fn default() -> Self {
        OrderedProperties { map: ConcurrentHashMap::new(8), keys: Vec::new(), lines: std::collections::HashMap::new() }
    }
}

/// The hooks `PropertyText` overrides.
pub trait PropertiesHooks {
    /// `getTerminators()`.
    fn get_terminators(&self) -> &'static str {
        "=: \t"
    }
    /// `removeBadChars(prop, string, isValue)`.
    fn remove_bad_chars(&self, _prop: &str, string: String, _is_value: bool) -> String {
        string
    }
}

impl<V: Clone> OrderedProperties<V> {
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.map.get(key)
    }

    /// `put(key, value)`.
    pub fn put(&mut self, key: &str, value: V) -> Option<V> {
        let old = self.map.put(key, value);
        if old.is_none() {
            self.keys.push(key.to_string());
        }
        old
    }

    /// `entrySet()`, in the backing map's order.
    pub fn entry_set(&self) -> Vec<(String, V)> {
        self.map.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    /// `load2(BufferedReader)` over the text of a UTF-8 file; `wrap` turns a parsed string into a value.
    pub fn load(&mut self, text: &str, hooks: &dyn PropertiesHooks, wrap: &dyn Fn(String) -> V) {
        let terminators = hooks.get_terminators();
        let lines = read_lines(text);
        let mut li = 0;
        let mut line_number = 0;
        while li < lines.len() {
            let mut line = lines[li].clone();
            li += 1;
            line_number += 1;
            if line_number == 1 && line.starts_with('\u{feff}') {
                line = line['\u{feff}'.len_utf8()..].to_string();
            }
            let units: Vec<u16> = jstr::to_utf16(&line);
            let len = units.len();
            let mut start = 0;
            while start < len && is_in(WHITESPACE, units[start]) {
                start += 1;
            }
            if line.trim_matches(|c: char| c <= ' ').is_empty() {
                continue;
            }
            if len == 0 || units[start] == b'!' as u16 || units[start] == b'#' as u16 || is_in(WHITESPACE, units[start]) {
                continue;
            }
            let mut buffer: Vec<u16> = units[start..].to_vec();
            let mut cur = units;
            while cur.len() > 1 && *cur.last().unwrap() == b'\\' as u16 {
                buffer.pop();
                if li < lines.len() {
                    let next = jstr::to_utf16(&lines[li]);
                    li += 1;
                    line_number += 1;
                    let mut ns = 0;
                    while ns < next.len() && is_in(WHITESPACE, next[ns]) {
                        ns += 1;
                    }
                    buffer.extend_from_slice(&next[ns..]);
                    cur = next;
                } else {
                    break;
                }
            }
            self.load_property(&buffer, line_number, terminators, hooks, wrap);
        }
    }

    fn load_property(&mut self, prop: &[u16], line_number: i32, terminators: &str, hooks: &dyn PropertiesHooks, wrap: &dyn Fn(String) -> V) {
        let prop_str = jstr::from_utf16(prop);
        let len = prop.len();
        let mut i = 0;
        while i < len {
            let c = prop[i];
            if c == b'\\' as u16 {
                i += 1;
            } else if is_in(terminators, c) {
                break;
            }
            i += 1;
        }
        let key = jstr::from_utf16(&prop[..i.min(len)]);
        let key = hooks.remove_bad_chars(&prop_str, key, false);
        let key = unescape(&key);
        let key = key.trim_matches(|c: char| c <= ' ').to_string();
        while i < len && is_in(WHITESPACE, prop[i]) {
            i += 1;
        }
        if i >= len {
            return;
        }
        if prop[i] == b':' as u16 || prop[i] == b'=' as u16 {
            i += 1;
            while i < len && is_in(WHITESPACE, prop[i]) {
                i += 1;
            }
        }
        let value_start = i;
        while i < len {
            let c = prop[i];
            if c == b'\\' as u16 {
                i += 1;
            } else if is_in(VALTERMINATORS, c) {
                break;
            }
            i += 1;
        }
        let value = jstr::from_utf16(&prop[value_start..i.min(len)]);
        let value = hooks.remove_bad_chars(&prop_str, value, true);
        let value = unescape(&value);
        if !self.map.contains_key(&key) {
            self.keys.push(key.clone());
        }
        self.map.put(&key, wrap(value));
        self.lines.insert(key, line_number);
    }
}

fn is_in(set: &str, c: u16) -> bool {
    c < 128 && set.as_bytes().contains(&(c as u8))
}

/// `BufferedReader.readLine()` over the whole text: `\n`, `\r` and `\r\n` end a line.
fn read_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut pending = false;
    while let Some(c) = chars.next() {
        pending = true;
        match c {
            '\n' => {
                out.push(std::mem::take(&mut cur));
                pending = false;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
                pending = false;
            }
            _ => cur.push(c),
        }
    }
    if pending {
        out.push(cur);
    }
    out
}

/// `unescape(string)`.
fn unescape(string: &str) -> String {
    let s = jstr::to_utf16(string);
    let mut out: Vec<u16> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let mut add = s[i];
        i += 1;
        if add == b'\\' as u16 {
            let Some(&a) = s.get(i) else { panic!("StringIndexOutOfBoundsException") };
            add = a;
            i += 1;
            if add == b'u' as u16 {
                let mut unicode: u32 = 0;
                for _ in 0..4 {
                    let Some(&a) = s.get(i) else { panic!("StringIndexOutOfBoundsException") };
                    i += 1;
                    match a {
                        0x30..=0x39 => unicode = (unicode << 4) + (a as u32 - 0x30),
                        0x61..=0x66 => unicode = (unicode << 4) + 10 + (a as u32 - 0x61),
                        0x41..=0x46 => unicode = (unicode << 4) + 10 + (a as u32 - 0x41),
                        _ => toolkit::log(CompilerMessage::error(format!("The encoding for the given string is malformed: {string}")), None, -1, -1, None),
                    }
                }
                add = unicode as u16;
            } else {
                add = match add {
                    0x74 => b'\t' as u16,
                    0x6e => b'\n' as u16,
                    0x72 => b'\r' as u16,
                    0x66 => 0x0c,
                    other => other,
                };
            }
        }
        out.push(add);
    }
    jstr::from_utf16(&out)
}
