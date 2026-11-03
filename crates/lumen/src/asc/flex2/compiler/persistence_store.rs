//! Port of `flex2.compiler.PersistenceStore`: the incremental compilation cache file. The writer takes the compile's
//! `FileSpec`, `SourceList`, `SourcePath`, `ResourceBundlePath` and sources; the reader returns the same data for
//! `CompilerAPI.loadCompilationUnits` to check against the current compile and turn back into sources and units.

use std::collections::HashMap;

use super::compilation_unit::{AuxGenerateInfo, CompilationUnit};
use super::source::{Source, SourceRef};
use super::toolkit::Warning;
use super::util::{MultiName, QName};
use crate::asc::util::jhash::JHashMap;

const MAJOR_VERSION: i32 = 4;
const MINOR_VERSION: i32 = 7;

pub const OBSOLETE_CACHE_FILE_FORMAT: &str = "Detected obsolete cache file format. The cache file will not be reused.";
pub const NO_SOURCE_PATH: &str = "SourcePath-based compilation data exists but a SourcePath is not provided.";

/// A constant pool entry, keyed as `PersistenceStore`'s `HashMap<Object, Integer>` compares them; `byte[]` keys
/// compare by identity, so each unit's bytecode gets its own entry.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Str(String),
    Strings(Vec<String>),
    Bytes(usize),
    QName(String, String),
    MultiName(Vec<String>, String),
}

enum Entry {
    Str(String),
    Strings(Vec<String>),
    Bytes(Vec<u8>),
    QName(String, String),
    MultiName(Vec<String>, String),
}

#[derive(Default)]
struct Pool {
    index: HashMap<Key, i32>,
    entries: Vec<Entry>,
}

impl Pool {
    fn put(&mut self, key: Key, entry: Entry) -> i32 {
        let i = self.entries.len() as i32;
        self.index.insert(key, i);
        self.entries.push(entry);
        i
    }

    /// `addString`.
    fn string(&mut self, s: &str) -> i32 {
        if let Some(&i) = self.index.get(&Key::Str(s.to_string())) {
            return i;
        }
        self.put(Key::Str(s.to_string()), Entry::Str(s.to_string()))
    }

    /// `addStrings`.
    fn strings(&mut self, a: &[String]) -> i32 {
        if let Some(&i) = self.index.get(&Key::Strings(a.to_vec())) {
            return i;
        }
        for s in a {
            self.string(s);
        }
        self.put(Key::Strings(a.to_vec()), Entry::Strings(a.to_vec()))
    }

    /// `addBytes`.
    fn bytes(&mut self, b: &[u8]) -> i32 {
        let id = self.entries.len();
        self.put(Key::Bytes(id), Entry::Bytes(b.to_vec()))
    }

    /// `addQName`.
    fn qname(&mut self, q: &QName) -> i32 {
        let key = Key::QName(q.get_namespace().to_string(), q.get_local_part().to_string());
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        self.string(q.get_namespace());
        self.string(q.get_local_part());
        self.put(key, Entry::QName(q.get_namespace().to_string(), q.get_local_part().to_string()))
    }

    /// `addMultiName`.
    fn multi_name(&mut self, m: &MultiName) -> i32 {
        let key = Key::MultiName(m.namespace_uri.clone(), m.local_part.clone());
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        self.strings(&m.namespace_uri);
        self.string(&m.local_part);
        self.put(key, Entry::MultiName(m.namespace_uri.clone(), m.local_part.clone()))
    }
}

#[derive(Default)]
struct Out {
    b: Vec<u8>,
}

impl Out {
    fn u8(&mut self, v: i32) {
        self.b.push(v as u8);
    }

    fn u32(&mut self, v: i32) {
        self.b.extend_from_slice(&v.to_be_bytes());
    }

    fn long(&mut self, v: i64) {
        self.b.extend_from_slice(&v.to_be_bytes());
    }

    fn bytes(&mut self, v: &[u8]) {
        self.b.extend_from_slice(v);
    }
}

/// `ResourceBundlePath` as the store reads it.
pub struct BundlePathData<'a> {
    pub locales: &'a [String],
    pub directories: Vec<(String, Vec<String>)>,
    /// Bundle name, source, and per locale the resource file and its path root names.
    pub sources: Vec<(String, SourceRef, Vec<(Option<String>, Option<String>)>)>,
}

/// What `PersistenceStore.write` is given.
pub struct StoreInput<'a> {
    pub file_spec_mime_types: &'a [&'a str],
    pub source_list: Option<(&'a [&'a str], Vec<String>, Vec<SourceRef>)>,
    pub source_path: Option<(&'a [&'a str], Vec<String>, Vec<(String, SourceRef)>)>,
    pub bundle_path: Option<(&'a [&'a str], BundlePathData<'a>)>,
    pub sources: &'a [SourceRef],
    pub checksums: [i32; 4],
    pub swc_def_signature_checksums: Option<&'a JHashMap<QName, i64>>,
    pub swc_file_checksums: Option<&'a JHashMap<String, i64>>,
    pub archive_file_checksums: Option<&'a JHashMap<String, i64>>,
    pub target_player_major_version: i32,
}

/// `writeSourceNames`' owner code.
fn source_name_owner(s: &Source) -> i32 {
    if s.is_source_list_owner() {
        1
    } else if s.is_source_path_owner() {
        2
    } else if s.is_resource_container_owner() {
        3
    } else if s.is_resource_bundle_path_owner() {
        4
    } else if s.is_swc_script_owner() {
        5
    } else if s.is_compiler_swc_context_owner() {
        6
    } else {
        7
    }
}

/// `PersistenceStore.write(...)`: the cache file's bytes, constant pool last.
pub fn write(input: &StoreInput) -> Vec<u8> {
    let mut pool = Pool::default();
    let mut out = Out::default();
    out.u32(MAJOR_VERSION);
    out.u32(MINOR_VERSION);
    for c in input.checksums {
        out.u32(c);
    }
    out.u32(0);
    let pointer = out.b.len();
    out.long(0);

    out.u8(1);
    out.u32(input.file_spec_mime_types.len() as i32);
    for m in input.file_spec_mime_types {
        let i = pool.string(m);
        out.u32(i);
    }
    out.u32(0);

    match &input.source_list {
        None => out.u8(0),
        Some((mimes, paths, sources)) => {
            out.u8(1);
            out.u32(mimes.len() as i32);
            for m in mimes.iter() {
                let i = pool.string(m);
                out.u32(i);
            }
            out.u32(paths.len() as i32);
            for p in paths {
                let i = pool.string(p);
                out.u32(i);
            }
            out.u32(sources.len() as i32);
            for s in sources {
                let i = pool.string(s.borrow().get_name());
                out.u32(i);
            }
        }
    }

    match &input.source_path {
        None => out.u8(0),
        Some((mimes, paths, sources)) => {
            out.u8(1);
            out.u32(mimes.len() as i32);
            for m in mimes.iter() {
                let i = pool.string(m);
                out.u32(i);
            }
            out.u32(paths.len() as i32);
            for p in paths {
                let i = pool.string(p);
                out.u32(i);
            }
            out.u32(sources.len() as i32);
            for (class_name, s) in sources {
                let i = pool.string(class_name);
                out.u32(i);
                let i = pool.string(s.borrow().get_name());
                out.u32(i);
            }
        }
    }

    match &input.bundle_path {
        None => out.u8(0),
        Some((mimes, bp)) => {
            out.u8(1);
            out.u32(mimes.len() as i32);
            for m in mimes.iter() {
                let i = pool.string(m);
                out.u32(i);
            }
            out.u32(bp.locales.len() as i32);
            for l in bp.locales {
                let i = pool.string(l);
                out.u32(i);
                let paths = bp.directories.iter().find(|(k, _)| k == l).map(|(_, p)| p.clone()).unwrap_or_default();
                out.u32(paths.len() as i32);
                for p in &paths {
                    let i = pool.string(p);
                    out.u32(i);
                }
            }
            out.u32(bp.sources.len() as i32);
            for (bundle_name, s, files) in &bp.sources {
                let i = pool.string(bundle_name);
                out.u32(i);
                let i = pool.string(s.borrow().get_name());
                out.u32(i);
                out.u32(files.len() as i32);
                for (f, r) in files {
                    let i = pool.string(f.as_deref().unwrap_or("null"));
                    out.u32(i);
                    let i = pool.string(r.as_deref().unwrap_or("null"));
                    out.u32(i);
                }
            }
        }
    }

    out.u32(input.sources.len() as i32);
    for s in input.sources {
        let s = s.borrow();
        let i = pool.string(s.get_name());
        out.u32(i);
        out.u8(source_name_owner(&s));
    }

    write_checksums_qname(&mut out, &mut pool, input.swc_def_signature_checksums);
    write_checksums_string(&mut out, &mut pool, input.swc_file_checksums);
    write_checksums_string(&mut out, &mut pool, input.archive_file_checksums);

    let c2: Vec<SourceRef> = input.source_list.as_ref().map(|(_, _, s)| s.clone()).unwrap_or_default();
    let c3: Vec<SourceRef> = input.source_path.as_ref().map(|(_, _, s)| s.iter().map(|(_, s)| s.clone()).collect()).unwrap_or_default();
    let c5: Vec<SourceRef> = input.bundle_path.as_ref().map(|(_, bp)| bp.sources.iter().map(|(_, s, _)| s.clone()).collect()).unwrap_or_default();
    out.u32((c2.len() + c3.len() + c5.len()) as i32);
    for s in c2.iter().chain(c3.iter()).chain(c5.iter()) {
        write_source(&mut out, &mut pool, &s.borrow());
        let u = s.borrow().get_compilation_unit();
        if let Some(u) = u {
            write_compilation_unit(&mut out, &mut pool, &u.borrow(), input.target_player_major_version);
        }
    }

    let offset = out.b.len() as i64;
    out.b[pointer..pointer + 8].copy_from_slice(&offset.to_be_bytes());
    write_constant_pool(&mut out, &mut pool);
    out.b
}

fn write_checksums_qname(out: &mut Out, pool: &mut Pool, m: Option<&JHashMap<QName, i64>>) {
    out.u32(m.map_or(0, |m| m.len()) as i32);
    for (q, ts) in m.into_iter().flat_map(|m| m.iter()) {
        let i = pool.qname(q);
        out.u32(i);
        out.long(*ts);
    }
}

fn write_checksums_string(out: &mut Out, pool: &mut Pool, m: Option<&JHashMap<String, i64>>) {
    out.u32(m.map_or(0, |m| m.len()) as i32);
    for (f, ts) in m.into_iter().flat_map(|m| m.iter()) {
        let i = pool.string(f);
        out.u32(i);
        out.long(*ts);
    }
}

/// `writeSource`.
fn write_source(out: &mut Out, pool: &mut Pool, s: &Source) {
    let unit = s.get_compilation_unit();
    let i = pool.string(s.get_name());
    out.u32(i);
    let i = pool.string(s.get_relative_path());
    out.u32(i);
    let i = pool.string(s.get_short_name());
    out.u32(i);
    let root = |pool: &mut Pool, out: &mut Out| {
        let i = pool.string(s.get_path_root().unwrap_or("null"));
        out.u32(i);
    };
    if s.is_source_list_owner() {
        out.u8(1);
        root(pool, out);
    } else if s.is_source_path_owner() {
        out.u8(2);
        root(pool, out);
    } else if s.is_resource_container_owner() {
        out.u8(3);
    } else if s.is_resource_bundle_path_owner() {
        out.u8(4);
        root(pool, out);
    } else if s.is_compiler_swc_context_owner() {
        out.u8(5);
    } else {
        out.u8(6);
    }
    out.u8(s.is_internal() as i32);
    out.u8(s.is_root() as i32);
    out.u8(s.is_debuggable() as i32);
    out.u8(unit.is_some() as i32);
    out.long(s.get_file_time());
    let sig = unit.as_ref().and_then(|u| u.borrow().signature_checksum);
    out.u8(sig.is_some() as i32);
    if let Some(c) = sig {
        out.long(c);
    }
    let includes = s.get_file_includes();
    out.u32(includes.len() as i32);
    for (f, t) in &includes {
        let i = pool.string(f);
        out.u32(i);
        out.long(*t);
    }
    let warnings = s.get_logger().map(|l| l.get_warnings()).unwrap_or_default();
    out.u32(warnings.len() as i32);
    for w in &warnings {
        for t in [&w.path, &w.warning, &w.source] {
            let i = pool.string(t.as_deref().unwrap_or(""));
            out.u32(i);
        }
        out.u32(w.line.unwrap_or(-1));
        out.u32(w.col.unwrap_or(-1));
        out.u32(w.error_code.unwrap_or(-1));
    }
}

fn write_map(out: &mut Out, pool: &mut Pool, m: &[(String, String)]) {
    out.u32(m.len() as i32);
    for (k, v) in m {
        let i = pool.string(k);
        out.u32(i);
        let i = pool.string(v);
        out.u32(i);
    }
}

/// `writeCompilationUnit`.
fn write_compilation_unit(out: &mut Out, pool: &mut Pool, u: &CompilationUnit, target_player_major_version: i32) {
    let i = pool.bytes(u.get_byte_codes());
    out.u32(i);
    out.u32(u.get_workflow());
    out.u32(u.get_state());
    out.u32(u.top_level_definitions.len() as i32);
    for q in u.top_level_definitions.iter() {
        let i = pool.qname(q);
        out.u32(i);
    }
    for h in [&u.inheritance_history, &u.type_history, &u.namespace_history, &u.expression_history] {
        let entries: Vec<(MultiName, QName)> = h.iter().map(|(m, q)| (m.clone(), q.clone())).collect();
        out.u32(entries.len() as i32);
        for (m, q) in &entries {
            let i = pool.multi_name(m);
            out.u32(i);
            let i = pool.qname(q);
            out.u32(i);
        }
    }
    match &u.aux_generate_info {
        Some(a) => {
            out.u8(1);
            for t in [&a.base_loader_class, &a.generate_loader_class, &a.window_class, &a.preloader_class] {
                let i = pool.string(t);
                out.u32(i);
            }
            out.u8(a.use_preloader as i32);
            for m in [&a.root_attributes, &a.root_attribute_embed_vars, &a.root_attribute_embed_names] {
                write_map(out, pool, m.as_deref().unwrap_or(&[]));
            }
        }
        None => out.u8(0),
    }
    match (&u.icon_file, &u.icon) {
        (Some(f), icon) => {
            out.u8(1);
            let i = pool.string(icon.as_deref().unwrap_or(""));
            out.u32(i);
            let i = pool.string(f);
            out.u32(i);
        }
        _ => out.u8(0),
    }
    if u.has_assets() {
        out.u32(u.assets.count() as i32);
        let mut frame = crate::asc::flex2::flash::swf::movie_encoder::FrameAssets::default();
        for (class_name, info) in u.assets.iter() {
            let i = pool.string(class_name);
            out.u32(i);
            let i = pool.string(info.path.as_ref().map_or("", |p| p.get_name()));
            out.u32(i);
            out.long(info.creation_time);
            frame.add_symbol_class(class_name, info.define_tag.clone());
            if info.define_tag.borrow().name.is_some() {
                frame.add_export(&info.define_tag);
            }
        }
        let swf = crate::asc::flex2::swf::encode_assets(target_player_major_version, &frame);
        out.u32(swf.len() as i32);
        out.bytes(&swf);
    } else {
        out.u32(0);
    }
}

/// `writeConstantPool`.
fn write_constant_pool(out: &mut Out, pool: &mut Pool) {
    let n = pool.entries.len();
    out.u32(n as i32);
    for i in 0..n {
        match &pool.entries[i] {
            Entry::Str(s) => {
                let b = s.as_bytes().to_vec();
                out.u8(1);
                out.u32(b.len() as i32);
                out.bytes(&b);
            }
            Entry::Strings(a) => {
                let a = a.clone();
                out.u8(2);
                out.u32(a.len() as i32);
                for s in &a {
                    let j = pool.string(s);
                    out.u32(j);
                }
            }
            Entry::Bytes(b) => {
                out.u8(3);
                out.u32(b.len() as i32);
                out.bytes(b);
            }
            Entry::QName(ns, l) => {
                let (ns, l) = (ns.clone(), l.clone());
                out.u8(4);
                let j = pool.string(&ns);
                out.u32(j);
                let j = pool.string(&l);
                out.u32(j);
            }
            Entry::MultiName(ns, l) => {
                let (ns, l) = (ns.clone(), l.clone());
                out.u8(5);
                let j = pool.strings(&ns);
                out.u32(j);
                let j = pool.string(&l);
                out.u32(j);
            }
        }
    }
}

/// A source as the cache recorded it.
#[derive(Clone)]
pub struct CachedSource {
    pub name: String,
    pub relative_path: String,
    pub short_name: String,
    pub owner: u8,
    pub path_root: Option<String>,
    pub is_internal: bool,
    pub is_root: bool,
    pub is_debuggable: bool,
    pub file_time: i64,
    pub signature_checksum: Option<i64>,
    pub includes: Vec<(String, i64)>,
    pub warnings: Vec<Warning>,
    pub unit: Option<CachedUnit>,
}

#[derive(Clone)]
pub struct CachedUnit {
    pub abc: Vec<u8>,
    pub workflow: i32,
    pub state: i32,
    pub top_level_definitions: Vec<QName>,
    pub inheritance_history: Vec<(MultiName, QName)>,
    pub type_history: Vec<(MultiName, QName)>,
    pub namespace_history: Vec<(MultiName, QName)>,
    pub expression_history: Vec<(MultiName, QName)>,
    pub aux_generate_info: Option<AuxGenerateInfo>,
    pub icon: Option<(String, String)>,
    /// Class name, asset path (empty for none) and creation time.
    pub assets: Vec<(String, String, i64)>,
    pub asset_swf: Vec<u8>,
}

/// The cache file's contents, as `read` meets them.
#[derive(Default)]
pub struct CacheData {
    pub checksums: [i32; 4],
    pub file_spec: Option<(Vec<String>, Vec<String>)>,
    pub source_list: Option<(Vec<String>, Vec<String>, Vec<String>)>,
    pub source_path: Option<(Vec<String>, Vec<String>, Vec<(String, String)>)>,
    pub bundle_path: Option<(Vec<String>, Vec<(String, Vec<String>)>, Vec<(String, String, Vec<(Option<String>, Option<String>)>)>)>,
    pub source_names: Vec<(String, Option<u8>)>,
    pub swc_def_signature_checksums: Vec<(QName, i64)>,
    pub swc_file_checksums: Vec<(String, i64)>,
    pub archive_file_checksums: Vec<(String, i64)>,
    pub sources: Vec<CachedSource>,
}

#[derive(Clone)]
enum Value {
    Str(String),
    Strings(Vec<String>),
    Bytes(Vec<u8>),
    QName(QName),
    MultiName(MultiName),
}

struct In<'a> {
    d: &'a [u8],
    p: usize,
    pool: Vec<Value>,
}

/// An `IOException`: `EOFException` (no message) or the store's own.
pub struct ReadError(pub Option<String>);

impl In<'_> {
    fn u8(&mut self) -> Result<i32, ReadError> {
        let v = *self.d.get(self.p).ok_or(ReadError(None))? as i32;
        self.p += 1;
        Ok(v)
    }

    fn u32(&mut self) -> Result<i32, ReadError> {
        let b = self.d.get(self.p..self.p + 4).ok_or(ReadError(None))?;
        self.p += 4;
        Ok(i32::from_be_bytes(b.try_into().unwrap()))
    }

    fn long(&mut self) -> Result<i64, ReadError> {
        let b = self.d.get(self.p..self.p + 8).ok_or(ReadError(None))?;
        self.p += 8;
        Ok(i64::from_be_bytes(b.try_into().unwrap()))
    }

    fn bytes(&mut self, n: i32) -> Result<Vec<u8>, ReadError> {
        let n = usize::try_from(n).map_err(|_| ReadError(None))?;
        let b = self.d.get(self.p..self.p + n).ok_or(ReadError(None))?.to_vec();
        self.p += n;
        Ok(b)
    }

    fn entry(&mut self) -> Result<Value, ReadError> {
        let i = self.u32()?;
        usize::try_from(i).ok().and_then(|i| self.pool.get(i).cloned()).ok_or(ReadError(None))
    }

    fn string(&mut self) -> Result<String, ReadError> {
        match self.entry()? {
            Value::Str(s) => Ok(s),
            _ => Err(ReadError(None)),
        }
    }

    fn qname(&mut self) -> Result<QName, ReadError> {
        match self.entry()? {
            Value::QName(q) => Ok(q),
            _ => Err(ReadError(None)),
        }
    }

    fn multi_name(&mut self) -> Result<MultiName, ReadError> {
        match self.entry()? {
            Value::MultiName(m) => Ok(m),
            _ => Err(ReadError(None)),
        }
    }

    fn strings(&mut self) -> Result<Vec<String>, ReadError> {
        let n = self.u32()?;
        (0..n).map(|_| self.string()).collect()
    }
}

/// The result of `read`'s first steps: `readVersion` failing throws, `readChecksum` failing returns -1.
pub enum ReadResult {
    Obsolete,
    ChecksumMismatch([i32; 4]),
    Data(Box<CacheData>),
}

/// `PersistenceStore.read`'s parsing, with `cmd_checksum` the current `checksums[1]`.
pub fn read(d: &[u8], cmd_checksum: i32) -> Result<ReadResult, ReadError> {
    let mut r = In { d, p: 0, pool: Vec::new() };
    let (major, minor) = (r.u32()?, r.u32()?);
    if major != MAJOR_VERSION || minor != MINOR_VERSION {
        return Ok(ReadResult::Obsolete);
    }
    let checksums = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
    let n = r.u32()?;
    r.bytes(n)?;
    if checksums[1] != cmd_checksum {
        return Ok(ReadResult::ChecksumMismatch(checksums));
    }
    let pool_offset = r.long()?;
    let start = r.p;
    r.p = usize::try_from(pool_offset).map_err(|_| ReadError(None))?;
    let n = r.u32()?;
    for _ in 0..n {
        let v = match r.u8()? {
            1 => {
                let n = r.u32()?;
                Value::Str(String::from_utf8_lossy(&r.bytes(n)?).into_owned())
            }
            2 => Value::Strings(r.strings()?),
            3 => {
                let n = r.u32()?;
                Value::Bytes(r.bytes(n)?)
            }
            4 => {
                let (ns, l) = (r.string()?, r.string()?);
                Value::QName(QName::new(&ns, &l))
            }
            5 => {
                let ns = match r.entry()? {
                    Value::Strings(a) => a,
                    _ => return Err(ReadError(None)),
                };
                let l = r.string()?;
                Value::MultiName(MultiName { namespace_uri: ns, local_part: l })
            }
            _ => return Err(ReadError(None)),
        };
        r.pool.push(v);
    }
    r.p = start;
    let mut data = CacheData { checksums, ..Default::default() };
    if r.u8()? == 1 {
        data.file_spec = Some((r.strings()?, r.strings()?));
    }
    if r.u8()? == 1 {
        data.source_list = Some((r.strings()?, r.strings()?, r.strings()?));
    }
    if r.u8()? == 1 {
        let (m, p) = (r.strings()?, r.strings()?);
        let n = r.u32()?;
        let mut s = Vec::new();
        for _ in 0..n {
            s.push((r.string()?, r.string()?));
        }
        data.source_path = Some((m, p, s));
    }
    if r.u8()? == 1 {
        let m = r.strings()?;
        let n = r.u32()?;
        let mut locales = Vec::new();
        for _ in 0..n {
            locales.push((r.string()?, r.strings()?));
        }
        let n = r.u32()?;
        let mut sources = Vec::new();
        let null = |s: String| if s == "null" { None } else { Some(s) };
        for _ in 0..n {
            let (b, s) = (r.string()?, r.string()?);
            let k = r.u32()?;
            let mut files = Vec::new();
            for _ in 0..k {
                files.push((null(r.string()?), null(r.string()?)));
            }
            sources.push((b, s, files));
        }
        data.bundle_path = Some((m, locales, sources));
    }
    let n = r.u32()?;
    for _ in 0..n {
        let name = r.string()?;
        let owner = if name != "null" { Some(r.u8()? as u8) } else { None };
        data.source_names.push((name, owner));
    }
    let n = r.u32()?;
    for _ in 0..n {
        data.swc_def_signature_checksums.push((r.qname()?, r.long()?));
    }
    for m in [&mut data.swc_file_checksums, &mut data.archive_file_checksums] {
        let n = r.u32()?;
        for _ in 0..n {
            m.push((r.string()?, r.long()?));
        }
    }
    let n = r.u32()?;
    for _ in 0..n {
        data.sources.push(read_source(&mut r)?);
    }
    Ok(ReadResult::Data(Box::new(data)))
}

fn read_source(r: &mut In) -> Result<CachedSource, ReadError> {
    let (name, relative_path, short_name) = (r.string()?, r.string()?, r.string()?);
    let owner = r.u8()? as u8;
    let path_root = if matches!(owner, 1 | 2 | 4) { Some(r.string()?) } else { None };
    let (is_internal, is_root, is_debuggable, has_unit) = (r.u8()? == 1, r.u8()? == 1, r.u8()? == 1, r.u8()? == 1);
    let file_time = r.long()?;
    let signature_checksum = if r.u8()? == 1 { Some(r.long()?) } else { None };
    let n = r.u32()?;
    let mut includes = Vec::new();
    for _ in 0..n {
        includes.push((r.string()?, r.long()?));
    }
    let n = r.u32()?;
    let mut warnings = Vec::new();
    let opt = |s: String| if s.is_empty() { None } else { Some(s) };
    let num = |v: i32| if v == -1 { None } else { Some(v) };
    for _ in 0..n {
        let (path, warning, source) = (opt(r.string()?), opt(r.string()?), opt(r.string()?));
        let (line, col, error_code) = (num(r.u32()?), num(r.u32()?), num(r.u32()?));
        warnings.push(Warning { path, warning, source, line, col, error_code });
    }
    let unit = if has_unit { Some(read_unit(r)?) } else { None };
    Ok(CachedSource { name, relative_path, short_name, owner, path_root, is_internal, is_root, is_debuggable, file_time, signature_checksum, includes, warnings, unit })
}

fn read_unit(r: &mut In) -> Result<CachedUnit, ReadError> {
    let abc = match r.entry()? {
        Value::Bytes(b) => b,
        _ => return Err(ReadError(None)),
    };
    let (workflow, state) = (r.u32()?, r.u32()?);
    let n = r.u32()?;
    let mut top_level_definitions = Vec::new();
    for _ in 0..n {
        top_level_definitions.push(r.qname()?);
    }
    let hist = |r: &mut In| -> Result<Vec<(MultiName, QName)>, ReadError> {
        let n = r.u32()?;
        (0..n).map(|_| Ok((r.multi_name()?, r.qname()?))).collect()
    };
    let (inheritance_history, type_history, namespace_history, expression_history) = (hist(r)?, hist(r)?, hist(r)?, hist(r)?);
    let aux_generate_info = if r.u8()? == 1 {
        let (base_loader_class, generate_loader_class, window_class, preloader_class) = (r.string()?, r.string()?, r.string()?, r.string()?);
        let use_preloader = r.u8()? == 1;
        let map = |r: &mut In| -> Result<Vec<(String, String)>, ReadError> {
            let n = r.u32()?;
            (0..n).map(|_| Ok((r.string()?, r.string()?))).collect()
        };
        let (a, b, c) = (map(r)?, map(r)?, map(r)?);
        Some(AuxGenerateInfo {
            generate_loader_class,
            base_loader_class,
            window_class,
            preloader_class,
            use_preloader,
            root_attributes: Some(a),
            root_attribute_embed_vars: Some(b),
            root_attribute_embed_names: Some(c),
        })
    } else {
        None
    };
    let icon = if r.u8()? == 1 { Some((r.string()?, r.string()?)) } else { None };
    let n = r.u32()?;
    let mut assets = Vec::new();
    let mut asset_swf = Vec::new();
    if n > 0 {
        for _ in 0..n {
            assets.push((r.string()?, r.string()?, r.long()?));
        }
        let size = r.u32()?;
        asset_swf = r.bytes(size)?;
    }
    Ok(CachedUnit { abc, workflow, state, top_level_definitions, inheritance_history, type_history, namespace_history, expression_history, aux_generate_info, icon, assets, asset_swf })
}
