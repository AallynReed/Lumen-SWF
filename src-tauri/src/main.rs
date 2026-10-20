#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::mem::ManuallyDrop;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use ffdec::export::{Exporter, Kind, Options};
use ffdec::info::{describe, Field};
use ffdec::tree::{self, Node};
use ffdec::{Compression, SaveOptions, Swf};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// An open file and its exporter, whose render caches survive between calls.
struct Doc {
    id: u32,
    path: PathBuf,
    swf: *mut Swf,
    exporter: ManuallyDrop<Exporter<'static>>,
}

impl Doc {
    fn new(id: u32, path: PathBuf, swf: Swf) -> Doc {
        let swf = Box::into_raw(Box::new(swf));
        let swf_ref: &'static Swf = unsafe { &*swf };
        Doc { id, path, swf, exporter: ManuallyDrop::new(Exporter::new(swf_ref)) }
    }

    fn swf(&self) -> &Swf {
        unsafe { &*self.swf }
    }

    fn exporter(&self) -> &Exporter<'_> {
        &self.exporter
    }

    fn mutate(&mut self, f: impl FnOnce(&mut Swf) -> Res<()>) -> Res<()> {
        unsafe {
            ManuallyDrop::drop(&mut self.exporter);
            let swf_mut = &mut *self.swf;
            f(swf_mut)?;
            let swf_ref: &'static Swf = &*self.swf;
            self.exporter = ManuallyDrop::new(Exporter::new(swf_ref));
        }
        Ok(())
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.exporter);
            drop(Box::from_raw(self.swf));
        }
    }
}

unsafe impl Send for Doc {}
unsafe impl Sync for Doc {}

#[derive(Default)]
struct Docs {
    open: Mutex<Vec<Doc>>,
    next_id: AtomicU32,
    /// Bytes of the last preview, fetched separately as a raw IPC response.
    preview: Mutex<Option<(u32, String, Vec<u8>)>>,
}

type Res<T> = Result<T, String>;

#[derive(Serialize)]
struct Opened {
    id: u32,
    name: String,
    path: String,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct TagInfo {
    fields: Vec<Field>,
    size: usize,
}

#[derive(Serialize)]
struct Bytes {
    offset: usize,
    data: Vec<u8>,
    total: usize,
}

fn with_doc<T>(docs: &Docs, id: u32, f: impl FnOnce(&Doc) -> Res<T>) -> Res<T> {
    let open = docs.open.lock();
    let doc = open.iter().find(|d| d.id == id).ok_or("document is not open")?;
    f(doc)
}

fn with_doc_mut<T>(docs: &Docs, id: u32, f: impl FnOnce(&mut Doc) -> Res<T>) -> Res<T> {
    let mut open = docs.open.lock();
    let doc = open.iter_mut().find(|d| d.id == id).ok_or("document is not open")?;
    f(doc)
}

fn find_tag<'a>(swf: &'a Swf, path: &str) -> Res<&'a ffdec::Tag> {
    let p = tree::parse_path(path).ok_or("bad tag path")?;
    swf.tag(&p).ok_or_else(|| "no such tag".to_string())
}

fn load(docs: &Docs, path: PathBuf) -> Res<Opened> {
    let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let swf = Swf::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    let id = docs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let opened = Opened {
        id,
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.display().to_string(),
        warnings: swf.warnings.clone(),
    };
    docs.open.lock().push(Doc::new(id, path, swf));
    Ok(opened)
}

#[tauri::command]
async fn open_dialog(app: tauri::AppHandle, docs: State<'_, Docs>) -> Res<Vec<Opened>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Flash files", &["swf", "gfx", "spl"])
        .add_filter("All files", &["*"])
        .blocking_pick_files()
        .unwrap_or_default();
    let mut out = Vec::new();
    for p in picked {
        out.push(load(&docs, p.into_path().map_err(|e| e.to_string())?)?);
    }
    Ok(out)
}

#[tauri::command]
fn open_path(docs: State<'_, Docs>, path: String) -> Res<Opened> {
    load(&docs, PathBuf::from(path))
}

#[tauri::command]
fn close_doc(docs: State<'_, Docs>, id: u32) {
    docs.open.lock().retain(|d| d.id != id);
}

#[tauri::command]
fn get_tree(docs: State<'_, Docs>, id: u32, raw: bool) -> Res<Vec<Node>> {
    with_doc(&docs, id, |d| Ok(if raw { tree::tag_list(d.swf()) } else { tree::resource_tree(d.swf()) }))
}

#[tauri::command]
fn header_info(docs: State<'_, Docs>, id: u32) -> Res<Vec<Field>> {
    with_doc(&docs, id, |d| {
        let s = d.swf();
        let h = &s.header;
        let f = |name: &str, value: String| Field { name: name.into(), value };
        let mut out = vec![
            f("File", d.path.display().to_string()),
            f("Signature", h.signature.clone()),
            f("Compression", format!("{:?}{}", h.compression, if h.encrypted { " (Harman encrypted)" } else { "" })),
            f("SWF version", h.version.to_string()),
            f("File size", format!("{} bytes", h.file_size)),
            f("Frame size", format!("{} × {} px", s.frame_rect.width_px(), s.frame_rect.height_px())),
            f("Frame rate", format!("{} fps", s.frame_rate)),
            f("Frame count", s.frame_count.to_string()),
            f("Tags", s.timeline.tags.len().to_string()),
            f("ActionScript", if s.is_as3() { "3".into() } else { "1/2".into() }),
        ];
        if h.gfx {
            out.push(f("Format", "Scaleform GFx".into()));
        }
        for w in &s.warnings {
            out.push(f("Warning", w.clone()));
        }
        Ok(out)
    })
}

#[tauri::command]
fn tag_info(docs: State<'_, Docs>, id: u32, path: String) -> Res<TagInfo> {
    with_doc(&docs, id, |d| {
        let t = find_tag(d.swf(), &path)?;
        Ok(TagInfo { fields: describe(t), size: t.body.len() })
    })
}

#[tauri::command]
fn tag_bytes(docs: State<'_, Docs>, id: u32, path: String, offset: usize, len: usize) -> Res<Bytes> {
    with_doc(&docs, id, |d| {
        let t = find_tag(d.swf(), &path)?;
        let start = offset.min(t.body.len());
        let end = (start + len.min(1 << 20)).min(t.body.len());
        Ok(Bytes { offset: start, data: t.body[start..end].to_vec(), total: t.body.len() })
    })
}

fn save_dialog(app: &tauri::AppHandle, name: &str, filter: (&str, &[&str])) -> Option<PathBuf> {
    app.dialog().file().set_file_name(name).add_filter(filter.0, filter.1).blocking_save_file()?.into_path().ok()
}

#[tauri::command]
async fn export_tag(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, path: String) -> Res<Option<String>> {
    let (body, name) = with_doc(&docs, id, |d| {
        let t = find_tag(d.swf(), &path)?;
        let stem = d.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok((t.body.clone(), format!("{stem}_{}_{}.bin", path.replace('/', "-"), t.name())))
    })?;
    let Some(out) = save_dialog(&app, &name, ("Binary", &["bin"])) else { return Ok(None) };
    std::fs::write(&out, body).map_err(|e| e.to_string())?;
    Ok(Some(out.display().to_string()))
}

#[tauri::command]
async fn save_as(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, compression: String, encrypt: bool) -> Res<Option<String>> {
    let compression = Compression::parse(&compression).ok_or("unknown compression")?;
    let (bytes, name) = with_doc(&docs, id, |d| {
        let bytes = d.swf().save(&SaveOptions { compression, encrypt }).map_err(|e| e.to_string())?;
        Ok((bytes, d.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
    })?;
    let Some(out) = save_dialog(&app, &name, ("SWF", &["swf"])) else { return Ok(None) };
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(Some(out.display().to_string()))
}

#[derive(Serialize)]
struct PreviewInfo {
    kind: &'static str,
    mime: &'static str,
    text: Option<String>,
    alt_text: Option<String>,
    note: Option<String>,
    size: usize,
    exportable: bool,
    frames: usize,
    frame: usize,
    frame_rate: f32,
}

/// A preview of a tag, or of the root timeline when `path` is empty.
#[tauri::command]
async fn preview_info(docs: State<'_, Docs>, id: u32, path: String, frame: usize, script: Option<String>) -> Res<Option<PreviewInfo>> {
    let p = if path.is_empty() { Vec::new() } else { tree::parse_path(&path).ok_or("bad tag path")? };
    let (info, data) = with_doc(&docs, id, |d| {
        let exportable = d.swf().tag(&p).is_some_and(|t| Kind::of(t.code).is_some());
        let preview = match &script {
            Some(s) => d.exporter().script_preview(&p, s).map(Some),
            None => d.exporter().preview(&p, frame),
        };
        match preview.map_err(|e| e.to_string())? {
            Some(pv) => Ok((
                Some(PreviewInfo {
                    kind: pv.kind,
                    mime: pv.mime,
                    text: pv.text,
                    alt_text: pv.alt_text,
                    note: pv.note,
                    size: pv.data.len(),
                    exportable,
                    frames: pv.frames,
                    frame: pv.frame,
                    frame_rate: pv.frame_rate,
                }),
                pv.data,
            )),
            None => Ok((None, Vec::new())),
        }
    })?;
    *docs.preview.lock() = Some((id, path, data));
    Ok(info)
}

#[tauri::command]
fn preview_data(docs: State<'_, Docs>, id: u32, path: String) -> tauri::ipc::Response {
    let cached = docs.preview.lock();
    let data = match &*cached {
        Some((i, p, d)) if *i == id && *p == path => d.clone(),
        _ => Vec::new(),
    };
    tauri::ipc::Response::new(data)
}

#[derive(serde::Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "camelCase", default)]
struct ExportOpts {
    png: bool,
    wav: bool,
    shapes_png: bool,
    frames_svg: bool,
    texts_svg: bool,
    zoom: f32,
    pcode: bool,
}

impl From<ExportOpts> for Options {
    fn from(o: ExportOpts) -> Options {
        Options { png_only: o.png, wav_only: o.wav, shapes_png: o.shapes_png, frames_svg: o.frames_svg, texts_svg: o.texts_svg, zoom: o.zoom, scripts_pcode: o.pcode }
    }
}

/// Exports one asset: a save dialog for a single file, a folder for several.
#[tauri::command]
async fn export_asset(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, path: String, script: Option<String>, options: ExportOpts) -> Res<Option<String>> {
    let p = tree::parse_path(&path).ok_or("bad tag path")?;
    let files = with_doc(&docs, id, |d| match &script {
        Some(s) => d.exporter().export_script(&p, s, options.pcode).map(|f| vec![f]).map_err(|e| e.to_string()),
        None => d.exporter().export(&p, options.into()).map_err(|e| e.to_string()),
    })?;
    match files.len() {
        0 => Err("this tag is not an exportable asset".into()),
        1 => {
            let file = &files[0];
            let ext = file.name.rsplit('.').next().unwrap_or("bin").to_string();
            let Some(out) = save_dialog(&app, &file.name, (&ext.to_uppercase(), &[&ext])) else { return Ok(None) };
            std::fs::write(&out, &file.data).map_err(|e| e.to_string())?;
            Ok(Some(out.display().to_string()))
        }
        n => {
            let Some(dir) = app.dialog().file().blocking_pick_folder().and_then(|d| d.into_path().ok()) else { return Ok(None) };
            for f in &files {
                let target = dir.join(&f.name);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&target, &f.data).map_err(|e| e.to_string())?;
            }
            Ok(Some(format!("{n} files to {}", dir.display())))
        }
    }
}

#[derive(Serialize)]
struct ExportReport {
    dir: String,
    written: usize,
    errors: Vec<String>,
}

#[tauri::command]
async fn export_all(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, kinds: Vec<String>, options: ExportOpts) -> Res<Option<ExportReport>> {
    let kinds: Vec<Kind> = kinds.iter().filter_map(|k| Kind::parse(k)).collect();
    if kinds.is_empty() {
        return Err("choose at least one asset type".into());
    }
    let Some(dir) = app.dialog().file().blocking_pick_folder().and_then(|d| d.into_path().ok()) else { return Ok(None) };
    with_doc(&docs, id, |d| {
        let (written, errors) = d.exporter().write_all(&dir, &kinds, options.into());
        Ok(Some(ExportReport { dir: dir.display().to_string(), written: written.len(), errors }))
    })
}

#[tauri::command]
fn startup_files() -> Vec<String> {
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
}

#[tauri::command]
async fn replace_asset(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, path: String) -> Res<Option<String>> {
    let p = tree::parse_path(&path).ok_or("bad tag path")?;
    let (code, tag_name) = with_doc(&docs, id, |d| {
        let t = d.swf().tag(&p).ok_or("tag not found")?;
        Ok((t.code, t.name()))
    })?;

    let filter: (&str, &[&str]) = match code {
        ffdec::tags::code::DEFINE_BITS
        | ffdec::tags::code::DEFINE_BITS_JPEG2
        | ffdec::tags::code::DEFINE_BITS_JPEG3
        | ffdec::tags::code::DEFINE_BITS_JPEG4
        | ffdec::tags::code::DEFINE_BITS_LOSSLESS
        | ffdec::tags::code::DEFINE_BITS_LOSSLESS2 => ("Image files", &["png", "jpg", "jpeg", "gif", "bmp"]),
        ffdec::tags::code::DEFINE_SOUND => ("Audio files", &["mp3", "wav", "bin"]),
        ffdec::tags::code::DEFINE_BINARY_DATA => ("Binary files", &["bin", "swf", "*"]),
        _ => ("All files", &["*"]),
    };

    let picked = app.dialog().file().add_filter(filter.0, filter.1).blocking_pick_file();
    let Some(file_path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    let data = std::fs::read(&file_path).map_err(|e| e.to_string())?;

    with_doc_mut(&docs, id, |d| {
        d.mutate(|swf| {
            match code {
                ffdec::tags::code::DEFINE_BITS
                | ffdec::tags::code::DEFINE_BITS_JPEG2
                | ffdec::tags::code::DEFINE_BITS_JPEG3
                | ffdec::tags::code::DEFINE_BITS_JPEG4
                | ffdec::tags::code::DEFINE_BITS_LOSSLESS
                | ffdec::tags::code::DEFINE_BITS_LOSSLESS2 => {
                    ffdec::replace::replace_image(swf, &p, &data).map_err(|e| e.to_string())?;
                }
                ffdec::tags::code::DEFINE_SOUND => {
                    ffdec::replace::replace_sound(swf, &p, &data).map_err(|e| e.to_string())?;
                }
                ffdec::tags::code::DEFINE_BINARY_DATA => {
                    ffdec::replace::replace_binary(swf, &p, &data).map_err(|e| e.to_string())?;
                }
                _ => {
                    swf.replace_tag_body(&p, &data).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })
    })?;

    Ok(Some(format!("Replaced {tag_name} with {}", file_path.file_name().unwrap_or_default().to_string_lossy())))
}

#[tauri::command]
async fn save_inplace(docs: State<'_, Docs>, id: u32) -> Res<String> {
    with_doc(&docs, id, |d| {
        let opts = SaveOptions {
            compression: d.swf().header.compression,
            encrypt: d.swf().header.encrypted,
        };
        let bytes = d.swf().save(&opts).map_err(|e| e.to_string())?;
        std::fs::write(&d.path, bytes).map_err(|e| e.to_string())?;
        Ok(format!("Saved to {}", d.path.display()))
    })
}

#[tauri::command]
async fn apply_script_edit(docs: State<'_, Docs>, id: u32, path: String, script: Option<String>, text: String) -> Res<String> {
    let p = if path.is_empty() { Vec::new() } else { tree::parse_path(&path).ok_or("bad tag path")? };
    with_doc_mut(&docs, id, |d| {
        d.mutate(|swf| {
            if let Some(name) = &script {
                // If it's an AS1/2 script
                let as2_list = ffdec::action::scripts::export_paths(&ffdec::action::scripts::tree(swf));
                for (n, src) in as2_list {
                    if n == *name || n.trim_start_matches("scripts/") == name.trim_start_matches("scripts/") {
                        let list = ffdec::action::asm::parse_pcode(&text, swf.header.version).map_err(|e| e.to_string())?;
                        swf.replace_as2_script(&src.tag, &list).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
                // If it's an AS3 entry
                let mut abcs = Vec::new();
                swf.walk(|path, t| {
                    if let Some((data, _)) = ffdec::abc::tag_abc(t) {
                        if let Ok(abc) = ffdec::abc::Abc::parse(data) {
                            abcs.push((path.to_vec(), abc));
                        }
                    }
                });
                for (abc_path, abc) in abcs {
                    for e in ffdec::abc::scripts::entries(&abc) {
                        if e.full_name() == *name {
                            let code = ffdec::abc::asm::assemble_instructions(&text, &abc).map_err(|e| e.to_string())?;
                            let mut new_abc = abc.clone();
                            if let Some(trait_idx) = e.trait_index {
                                let tr = &new_abc.scripts[e.script].traits[trait_idx];
                                let method_opt = match &tr.data {
                                    ffdec::abc::TraitData::Function { method, .. }
                                    | ffdec::abc::TraitData::Method { method, .. }
                                    | ffdec::abc::TraitData::Getter { method, .. }
                                    | ffdec::abc::TraitData::Setter { method, .. } => Some(*method),
                                    ffdec::abc::TraitData::Class { class, .. } => Some(new_abc.instances[*class].iinit),
                                    _ => None,
                                };
                                if let Some(m) = method_opt {
                                    if let Some(b_idx) = new_abc.body_of.get(m).copied().flatten() {
                                        new_abc.bodies[b_idx].code = code;
                                        swf.replace_as3_script(&abc_path, &new_abc.to_bytes()).map_err(|e| e.to_string())?;
                                        return Ok(());
                                    }
                                }
                            } else {
                                let init = new_abc.scripts[e.script].init;
                                if let Some(b_idx) = new_abc.body_of.get(init).copied().flatten() {
                                    new_abc.bodies[b_idx].code = code;
                                    swf.replace_as3_script(&abc_path, &new_abc.to_bytes()).map_err(|e| e.to_string())?;
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                return Err(format!("could not find script '{name}'"));
            } else {
                let tag = swf.tag(&p).ok_or("tag not found")?;
                if tag.code == ffdec::tags::code::DO_ACTION || tag.code == ffdec::tags::code::DO_INIT_ACTION {
                    let list = ffdec::action::asm::parse_pcode(&text, swf.header.version).map_err(|e| e.to_string())?;
                    swf.replace_as2_script(&p, &list).map_err(|e| e.to_string())?;
                } else {
                    return Err("editing this tag type is not supported directly".into());
                }
            }
            Ok(())
        })
    })?;
    Ok("Applied script update".into())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Docs::default())
        .invoke_handler(tauri::generate_handler![
            open_dialog,
            open_path,
            close_doc,
            get_tree,
            header_info,
            tag_info,
            tag_bytes,
            export_tag,
            save_as,
            preview_info,
            preview_data,
            export_asset,
            export_all,
            startup_files,
            replace_asset,
            save_inplace,
            apply_script_edit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rust-FFDec");
}
