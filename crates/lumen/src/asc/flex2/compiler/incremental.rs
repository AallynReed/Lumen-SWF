//! The incremental half of `flex2.compiler.CompilerAPI`: `loadCompilationUnits` (the cache's sources and units put
//! back into the `SourceList`, `SourcePath` and `ResourceBundlePath`), `persistCompilationUnits` and
//! `validateCompilationUnits`, which drops what changed since the cache was written.

use std::path::Path;
use std::rc::Rc;

use super::compiler_swc_context::CompilerSwcContext;
use super::configuration::Configuration;
use super::i18n::resource_file::ResourceBundlePath;
use super::persistence_store::{self, BundlePathData, CacheData, CachedSource, CachedUnit, StoreInput};
use super::source::{canonical_path, Owner, Source, SourceRef, VirtualFile};
use super::source_path::{SourceList, SourcePath};
use super::toolkit::{self, LocalLogger};
use super::util::{mime_mappings, MultiName, Name, QName};
use crate::asc::flex2::tools::swc_checksums::SwcChecksums;
use crate::asc::util::jhash::{JHashMap, JHashSet};

/// `WebTierAPI.getFileSpecMimeTypes()`.
pub const FILE_SPEC_MIME_TYPES: [&str; 5] = [mime_mappings::AS, mime_mappings::MXML, mime_mappings::FXG, mime_mappings::CSS, mime_mappings::ABC];
/// `WebTierAPI.getSourcePathMimeTypes()`.
pub const SOURCE_PATH_MIME_TYPES: [&str; 3] = [mime_mappings::AS, mime_mappings::MXML, mime_mappings::FXG];
/// `ResourceBundlePath`'s mime types.
pub const BUNDLE_PATH_MIME_TYPES: [&str; 1] = [mime_mappings::PROPERTIES];

/// `CompilerAPI.FailedToMatchCacheFile`.
pub fn failed_to_match(cache_name: &str) -> String {
    format!("Failed to match the compile target with {cache_name}. The cache file will not be reused.")
}

fn same(a: &[String], b: &[&str]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y)
}

fn paths(dirs: &[std::path::PathBuf]) -> Vec<String> {
    dirs.iter().map(|d| canonical_path(d)).collect()
}

/// `PathResolver.resolve(name)`, or a `DeletedFile` (here a local file that does not exist).
fn resolve_file(name: &str) -> VirtualFile {
    if Path::new(name).exists() {
        VirtualFile::local(Path::new(name))
    } else {
        VirtualFile::Local { name: name.to_string(), last_modified: -1 }
    }
}

/// `PersistenceStore.read`'s checks and `readCompilationUnit`s: the cached sources join the compile's containers.
/// Returns the unit count, or the negative code `read` returns on a mismatch.
pub fn load_compilation_units(data: &CacheData, source_list: &mut SourceList, source_path: &mut SourcePath, bundle_path: &mut ResourceBundlePath, swc_checksums: &mut SwcChecksums) -> i32 {
    if let Some((mimes, sources)) = &data.file_spec {
        if !same(mimes, &FILE_SPEC_MIME_TYPES) || !sources.is_empty() {
            return -2;
        }
    }
    if let Some((mimes, _, sources)) = &data.source_list {
        let current: Vec<String> = source_list.sources().iter().map(|s| s.borrow().get_name().to_string()).collect();
        if !same(mimes, &SOURCE_PATH_MIME_TYPES) || *sources != current {
            return -3;
        }
    }
    let mut mappings: JHashMap<String, String> = JHashMap::new();
    if let Some((mimes, dirs, sources)) = &data.source_path {
        if !same(mimes, source_path.get_mime_types()) || *dirs != paths(source_path.get_paths()) {
            return -4;
        }
        for (class_name, file_name) in sources {
            mappings.insert(file_name.clone(), class_name.clone());
        }
    }
    let mut rb_mappings: JHashMap<String, (String, Vec<(Option<String>, Option<String>)>)> = JHashMap::new();
    if let Some((mimes, locales, sources)) = &data.bundle_path {
        let target = bundle_path.get_locales();
        let dirs = bundle_path.get_resource_bundle_paths();
        if !same(mimes, &BUNDLE_PATH_MIME_TYPES) || locales.len() != target.len() || locales.iter().zip(target).any(|((l, _), t)| l != t) || dirs.len() != locales.len() {
            return -5;
        }
        for (i, (_, p)) in locales.iter().enumerate() {
            if *p != paths(&dirs[i].1) {
                return -5;
            }
        }
        for (bundle, file, files) in sources {
            rb_mappings.insert(file.clone(), (bundle.clone(), files.clone()));
        }
    }
    if let Some(m) = &mut swc_checksums.swc_def_signature_checksums {
        for (q, ts) in &data.swc_def_signature_checksums {
            m.insert(q.clone(), *ts);
        }
    }
    if let Some(m) = &mut swc_checksums.swc_file_checksums {
        for (f, ts) in &data.swc_file_checksums {
            m.insert(f.clone(), *ts);
        }
    }
    for (f, ts) in &data.archive_file_checksums {
        swc_checksums.archive_file_checksums.insert(f.clone(), *ts);
    }
    for cs in &data.sources {
        let s = match cs.owner {
            1 => {
                let s = source_list.sources().into_iter().find(|s| s.borrow().get_name() == cs.name);
                if let Some(s) = &s {
                    populate(s, cs);
                }
                s
            }
            2 => {
                let class_name = mappings.get(cs.name.as_str()).cloned();
                match class_name {
                    Some(c) if !source_path.sources().contains_key(c.as_str()) => {
                        let s = Source::new(resolve_file(&cs.name), cs.path_root.clone(), &cs.relative_path, &cs.short_name, Owner::SourcePath, cs.is_internal, cs.is_root, cs.is_debuggable);
                        populate(&s, cs);
                        source_path.sources_mut().insert(c, s.clone());
                        Some(s)
                    }
                    _ => None,
                }
            }
            4 => {
                let entry = rb_mappings.get(cs.name.as_str()).cloned();
                entry.map(|(bundle, files)| {
                    let s = bundle_path.restore_source(&bundle, &cs.name, &files, cs.path_root.clone(), &cs.relative_path, &cs.short_name);
                    populate(&s, cs);
                    s
                })
            }
            _ => None,
        };
        if let (Some(s), Some(cu)) = (&s, &cs.unit) {
            restore_unit(s, cs, cu);
        }
    }
    data.sources.len() as i32
}

/// `Source.populateSource`/`newSource`'s fields.
fn populate(s: &SourceRef, cs: &CachedSource) {
    let mut b = s.borrow_mut();
    b.set_file_time(cs.file_time);
    b.set_path_root(cs.path_root.clone());
    b.set_flags(&cs.relative_path, &cs.short_name, cs.is_internal, cs.is_root, cs.is_debuggable);
    b.set_file_include_times(cs.includes.clone());
    if !cs.warnings.is_empty() {
        let l = LocalLogger::restored(cs.warnings.clone());
        b.set_logger(Some(l));
    } else {
        b.set_logger(None);
    }
    drop(b);
    if let Some(l) = s.borrow().get_logger() {
        l.set_source(s);
    }
}

/// `readCompilationUnit`'s unit half.
fn restore_unit(s: &SourceRef, cs: &CachedSource, cu: &CachedUnit) {
    let unit = Source::new_compilation_unit(s, None);
    let mut u = unit.borrow_mut();
    u.signature_checksum = cs.signature_checksum;
    u.bytes = cu.abc.clone();
    u.restore_state(cu.workflow, cu.state);
    for q in &cu.top_level_definitions {
        u.top_level_definitions.add(q.clone());
    }
    for (m, q) in &cu.inheritance_history {
        u.inheritance_history.put(m.clone(), q.clone());
        u.inheritance.insert(Name::Q(q.clone()));
    }
    for (m, q) in &cu.type_history {
        u.type_history.put(m.clone(), q.clone());
        u.types.insert(Name::Q(q.clone()));
    }
    for (m, q) in &cu.namespace_history {
        u.namespace_history.put(m.clone(), q.clone());
        u.namespaces.insert(Name::Q(q.clone()));
    }
    for (m, q) in &cu.expression_history {
        u.expression_history.put(m.clone(), q.clone());
        u.expressions.insert(Name::Q(q.clone()));
    }
    u.aux_generate_info = cu.aux_generate_info.clone();
    if let Some((icon, file)) = &cu.icon {
        u.icon = Some(icon.clone());
        u.icon_file = Path::new(file).exists().then(|| file.clone());
    }
    if !cu.assets.is_empty() {
        let decoded = crate::asc::flex2::flash::swf::tag_decoder::decode_lenient(&cu.asset_swf);
        for (class_name, tag) in decoded.symbol_classes {
            let Some((_, path, time)) = cu.assets.iter().find(|(c, _, _)| *c == class_name) else { continue };
            let path = (!path.is_empty()).then(|| resolve_file(path));
            u.assets.add(&class_name, super::assets::AssetInfo::new(tag, path, *time, None));
        }
    }
}

/// `persistCompilationUnits(...)`: the cache bytes for this compile.
pub fn persist_compilation_units(source_list: &SourceList, source_path: &SourcePath, bundle_path: Option<&ResourceBundlePath>, sources: &[SourceRef], swc_checksums: &SwcChecksums, target_player_major_version: i32) -> Vec<u8> {
    let bp = bundle_path.map(|b| {
        let directories = b.get_resource_bundle_paths().iter().map(|(l, d)| (l.clone(), paths(d))).collect();
        let sources = b
            .sources()
            .iter()
            .map(|(name, s)| {
                let rf = super::i18n::resource_file::resource_file_of(s);
                let files = rf.map(|r| {
                    let (f, r) = r.get_resource_files();
                    f.into_iter().zip(r).collect()
                });
                (name.clone(), s.clone(), files.unwrap_or_default())
            })
            .collect();
        (&BUNDLE_PATH_MIME_TYPES[..], BundlePathData { locales: b.get_locales(), directories, sources })
    });
    let input = StoreInput {
        file_spec_mime_types: &FILE_SPEC_MIME_TYPES,
        source_list: Some((&SOURCE_PATH_MIME_TYPES[..], paths(source_list.get_paths()), source_list.sources())),
        source_path: Some((source_path.get_mime_types(), paths(source_path.get_paths()), source_path.sources().iter().map(|(k, v)| (k.clone(), v.clone())).collect())),
        bundle_path: bp,
        sources,
        checksums: swc_checksums.checksums,
        swc_def_signature_checksums: swc_checksums.swc_def_signature_checksums.as_ref(),
        swc_file_checksums: swc_checksums.swc_file_checksums.as_ref(),
        archive_file_checksums: Some(&swc_checksums.archive_file_checksums),
        target_player_major_version,
    };
    persistence_store::write(&input)
}

/// `CompilerAPI.computeSignatureChecksum`: the source parsed again on its own for its signature.
fn compute_signature_checksum(configuration: &Rc<Configuration>, s: &SourceRef) -> Option<i64> {
    use super::as3::as3_compiler::{As3Compiler, SubCompiler};
    let mut asc = As3Compiler::new(configuration.clone());
    asc.add_compiler_extension(Box::new(super::as3::signature_extension::SignatureExtension::new(false, None)));
    let tmp = {
        let b = s.borrow();
        let t = Source::new(b.get_backing_file().clone(), b.get_path_root().map(str::to_string), b.get_relative_path(), b.get_short_name(), b.get_owner().clone(), b.is_internal(), b.is_root(), b.is_debuggable());
        t.borrow_mut().set_file_time(b.get_file_time());
        t.borrow_mut().set_file_include_times(b.get_file_includes());
        t
    };
    let original = toolkit::get_logger();
    let local = LocalLogger::new();
    local.disconnect();
    toolkit::set_logger(Some(local));
    let mut symbol_table = super::symbol_table::SymbolTable::new(configuration);
    let u = asc.preprocess(tmp).and_then(|t| asc.parse1(&t, &mut symbol_table));
    toolkit::set_logger(original);
    u.and_then(|u| u.borrow().signature_checksum)
}

fn reason(map: &mut JHashMap<String, String>, s: &SourceRef, text: impl Into<String>) {
    map.insert(s.borrow().get_name().to_string(), text.into());
}

/// `toQName(name)`.
fn to_qname(n: &Name) -> Option<QName> {
    match n {
        Name::Q(q) => Some(q.clone()),
        Name::M(m) if m.namespace_uri.len() == 1 => Some(QName::new(&m.namespace_uri[0], &m.local_part)),
        _ => None,
    }
}

/// `MultiName.toString()`.
fn multi_name_string(m: &MultiName) -> String {
    format!("[{}]::{}", m.namespace_uri.join(", "), m.local_part)
}

/// `validateCompilationUnits(...)` for an application: the number of updated and affected sources, whose units
/// are dropped after their reasons are logged.
pub fn validate_compilation_units(source_list: &SourceList, source_path: &mut SourcePath, bundle_path: Option<&ResourceBundlePath>, swc_context: &CompilerSwcContext, configuration: &Rc<Configuration>) -> i32 {
    let strict = configuration.strict;
    let mut updated: JHashMap<String, SourceRef> = JHashMap::new();
    let mut updated_stable: JHashMap<String, SourceRef> = JHashMap::new();
    let mut affected: JHashMap<String, SourceRef> = JHashMap::new();
    let mut deleted: JHashMap<QName, SourceRef> = JHashMap::new();
    let mut reasons: JHashMap<String, String> = JHashMap::new();
    let mut q_names: JHashMap<QName, SourceRef> = JHashMap::new();
    let mut include_updated: JHashSet<String> = JHashSet::new();
    let mut dependents: JHashMap<QName, JHashMap<String, SourceRef>> = JHashMap::new();

    // `Set<Source> sources`: a HashSet, whose order follows the names' hashes.
    let mut sources: JHashMap<String, SourceRef> = JHashMap::new();
    let all: Vec<SourceRef> = source_list.sources().into_iter().chain(source_path.sources().values().cloned()).chain(bundle_path.map(|b| b.sources().values().cloned().collect::<Vec<_>>()).unwrap_or_default()).collect();
    for s in all {
        let name = s.borrow().get_name().to_string();
        if !sources.contains_key(name.as_str()) {
            sources.insert(name, s);
        }
    }
    let list = |m: &JHashMap<String, SourceRef>| m.values().cloned().collect::<Vec<_>>();

    for s in list(&sources) {
        let Some(u) = s.borrow().get_compilation_unit() else { continue };
        for f in s.borrow().get_updated_file_includes() {
            include_updated.insert(f);
        }
        for q in u.borrow().top_level_definitions.iter() {
            q_names.insert(q.clone(), s.clone());
            dependents.insert(q.clone(), JHashMap::new());
        }
    }
    for s in list(&sources) {
        let Some(u) = s.borrow().get_compilation_unit() else { continue };
        let u = u.borrow();
        let name = s.borrow().get_name().to_string();
        for set in [&u.inheritance, &u.namespaces, &u.types, &u.expressions] {
            for n in set.iter() {
                if let Some(q) = to_qname(n) {
                    if let Some(m) = dependents.get_mut(&q) {
                        m.insert(name.clone(), s.clone());
                    }
                }
            }
        }
    }

    for s in list(&sources) {
        let u = s.borrow().get_compilation_unit();
        let name = s.borrow().get_name().to_string();
        let not_done = u.as_ref().is_some_and(|u| !u.borrow().is_done() && !u.borrow().has_type_info);
        if s.borrow().has_error() || (!s.borrow().is_internal() && not_done) {
            affected.insert(name.clone(), s.clone());
            reason(&mut reasons, &s, "The source file wasn't fully compiled.");
            sources.remove(name.as_str());
        } else if !s.borrow().exists() {
            updated.insert(name.clone(), s.clone());
            reason(&mut reasons, &s, "The source file no longer exists.");
            if let Some(u) = &u {
                for q in u.borrow().top_level_definitions.iter() {
                    deleted.insert(q.clone(), s.clone());
                }
            }
            sources.remove(name.as_str());
        } else if s.borrow().is_updated() {
            let mut stable = false;
            let mime = s.borrow().get_mime_type();
            if let Some(u) = &u {
                if !configuration.disable_incremental_optimizations && mime != Some(mime_mappings::MXML) {
                    if let Some(persisted) = u.borrow().signature_checksum {
                        let current = compute_signature_checksum(configuration, &s);
                        stable = current == Some(persisted);
                    }
                }
            }
            if stable {
                updated_stable.insert(name.clone(), s.clone());
            } else {
                updated.insert(name.clone(), s.clone());
            }
            reason(&mut reasons, &s, "The source file or one of the included files has been updated.");
            sources.remove(name.as_str());
        } else if u.as_ref().is_some_and(|u| u.borrow().has_assets() && assets_updated(&u.borrow().assets)) {
            updated.insert(name.clone(), s.clone());
            reason(&mut reasons, &s, "One of the assets has been updated.");
            sources.remove(name.as_str());
        }
    }

    for s in deleted.values() {
        if s.borrow().is_source_path_owner() {
            source_path.remove_source(s);
        }
    }

    for s in list(&sources) {
        if s.borrow().is_source_path_owner() && !source_path.check_preference(&s) {
            let name = s.borrow().get_name().to_string();
            affected.insert(name.clone(), s.clone());
            reason(&mut reasons, &s, "The source file is no longer the source path's first preference.");
            sources.remove(name.as_str());
        }
    }

    for s in list(&sources) {
        let Some(u) = s.borrow().get_compilation_unit() else { continue };
        let name = s.borrow().get_name().to_string();
        let mut dependencies: JHashSet<Name> = JHashSet::new();
        {
            let u = u.borrow();
            for set in [&u.inheritance, &u.namespaces, &u.expressions, &u.types] {
                for n in set.iter() {
                    dependencies.insert(n.clone());
                }
            }
        }
        dependencies.insert(Name::Q(QName::new("", "Namespace")));
        let mut valid = true;
        let has_type_info = u.borrow().has_type_info;
        for d in dependencies.iter() {
            if let Some(q) = to_qname(d) {
                match q_names.get(&q) {
                    Some(ds) => {
                        let dep_ti = ds.borrow().get_compilation_unit().is_some_and(|du| du.borrow().has_type_info);
                        if has_type_info && !dep_ti && !ds.borrow().is_internal() {
                            reason(&mut reasons, &s, format!("Dependency '{}' not cached.", name_string(d)));
                            valid = false;
                        }
                    }
                    None if has_type_info => {
                        reason(&mut reasons, &s, format!("Dependency '{}' not cached.", name_string(d)));
                        valid = false;
                    }
                    None => {}
                }
            }
            if !valid {
                affected.insert(name.clone(), s.clone());
                sources.remove(name.as_str());
                break;
            }
        }
        valid = valid && strict;
        let (packages, definitions): (Vec<String>, Vec<QName>) = {
            let u = u.borrow();
            (u.import_package_statements.iter().cloned().collect(), u.import_definition_statements.iter().cloned().collect())
        };
        for p in packages {
            if !valid {
                break;
            }
            if !(source_path.has_package(&p) || swc_context.has_package(&p)) {
                affected.insert(name.clone(), s.clone());
                reason(&mut reasons, &s, format!("The import statement, {p}, is no longer valid."));
                sources.remove(name.as_str());
                valid = false;
            }
        }
        for d in definitions {
            if !valid {
                break;
            }
            if !(source_path.has_definition(d.get_namespace(), d.get_local_part()) || swc_context.has_definition(&d)) {
                affected.insert(name.clone(), s.clone());
                reason(&mut reasons, &s, format!("The import statement, {d}, is no longer valid."));
                sources.remove(name.as_str());
                valid = false;
            }
        }
    }

    if strict {
        let mut updated_and_affected: JHashMap<String, SourceRef> = JHashMap::new();
        updated_and_affected.put_all(updated.iter().map(|(k, v)| (k.clone(), v.clone())), updated.len());
        let n = affected.len();
        updated_and_affected.put_all(affected.iter().map(|(k, v)| (k.clone(), v.clone())), n);
        for s in list(&updated_and_affected) {
            dependent_file_modified(&s, &dependents, &updated, &mut affected, &mut reasons, &mut sources);
        }
    }

    for f in include_updated.iter() {
        toolkit::log_info(&format!("Updated: {f}"));
    }

    let mut affected_count = affected.len() as i32;
    log_reason_and_remove(&affected, &reasons, &include_updated, swc_context);
    log_reason_and_remove(&updated, &reasons, &include_updated, swc_context);
    log_reason_and_remove(&updated_stable, &reasons, &include_updated, swc_context);
    affected.clear();

    for s in list(&sources) {
        let Some(u) = s.borrow().get_compilation_unit() else { continue };
        let history: Vec<(MultiName, QName)> = u.borrow().inheritance_history.iter().map(|(m, q)| (m.clone(), q.clone())).collect();
        for (m, q) in history {
            match validate_multi_name(&m, &q, source_path) {
                Ok(true) => {}
                Ok(false) => {
                    let name = s.borrow().get_name().to_string();
                    affected.insert(name.clone(), s.clone());
                    reason(&mut reasons, &s, format!("The meaning of the multiname {} has changed from '{}' to something else.", multi_name_string(&m), q));
                    sources.remove(name.as_str());
                }
                Err(e) => {
                    let name = s.borrow().get_name().to_string();
                    affected.insert(name.clone(), s.clone());
                    reason(&mut reasons, &s, e);
                    sources.remove(name.as_str());
                }
            }
        }
    }
    affected_count += affected.len() as i32;
    log_reason_and_remove(&affected, &reasons, &include_updated, swc_context);

    let update_count = (updated.len() + updated_stable.len()) as i32;
    if update_count + affected_count > 0 {
        toolkit::log_info(&format!("Files changed: {update_count} Files affected: {affected_count}"));
    }
    update_count + affected_count
}

fn name_string(n: &Name) -> String {
    match n {
        Name::Q(q) => q.to_string(),
        Name::M(m) => multi_name_string(m),
    }
}

/// `Assets.isUpdated()`.
fn assets_updated(a: &super::assets::Assets) -> bool {
    a.iter().any(|(_, info)| info.path.as_ref().is_some_and(|p| info.creation_time != p.get_last_modified()))
}

/// `dependentFileModified(...)`.
fn dependent_file_modified(
    s: &SourceRef,
    dependents: &JHashMap<QName, JHashMap<String, SourceRef>>,
    updated: &JHashMap<String, SourceRef>,
    affected: &mut JHashMap<String, SourceRef>,
    reasons: &mut JHashMap<String, String>,
    sources: &mut JHashMap<String, SourceRef>,
) {
    let Some(u) = s.borrow().get_compilation_unit() else { return };
    let defs: Vec<QName> = u.borrow().top_level_definitions.iter().cloned().collect();
    let location = s.borrow().get_name().to_string();
    for q in defs {
        let Some(m) = dependents.get(&q) else { continue };
        for (k, d) in m.iter() {
            if !updated.contains_key(k.as_str()) && !affected.contains_key(k.as_str()) {
                affected.insert(k.clone(), d.clone());
                reasons.insert(k.clone(), format!("Its dependent file, {location}, has been updated."));
                sources.remove(d.borrow().get_name());
                dependent_file_modified(d, dependents, updated, affected, reasons, sources);
            }
        }
    }
}

/// `logReasonAndRemoveCompilationUnit(map, reasons, includeUpdated, swcContext)`.
fn log_reason_and_remove(map: &JHashMap<String, SourceRef>, reasons: &JHashMap<String, String>, include_updated: &JHashSet<String>, _swc_context: &CompilerSwcContext) {
    for (name, s) in map.iter() {
        for (f, _) in s.borrow().get_file_includes() {
            if !include_updated.contains(f.as_str()) {
                toolkit::log_info(&format!("Updated: {f}"));
            }
        }
        s.borrow_mut().remove_compilation_unit();
        toolkit::log_info(&format!("Recompile: {name}"));
        toolkit::log_info(&format!("Reason: {}", reasons.get(name.as_str()).cloned().unwrap_or_else(|| "null".into())));
    }
}

/// `validateMultiName(multiName, qName, sourcePath)`.
fn validate_multi_name(m: &MultiName, q: &QName, source_path: &mut SourcePath) -> Result<bool, String> {
    for ns in &m.namespace_uri {
        let s = source_path.find_source(ns, &m.local_part).map_err(|e| e.message)?;
        let Some(s) = s else { continue };
        let mut ns = ns.clone();
        if let Some(u) = s.borrow().get_compilation_unit() {
            if let Some(first) = u.borrow().top_level_definitions.first() {
                ns = first.get_namespace().to_string();
            }
        }
        if !(q.get_namespace() == ns && q.get_local_part() == m.local_part) {
            return Ok(false);
        }
    }
    Ok(true)
}
