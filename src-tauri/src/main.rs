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
use tauri::{Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// An open file and its exporter, whose render caches survive between calls.
struct Doc {
    id: u32,
    path: PathBuf,
    /// The entry of a ZIP, SWC or other file the SWF came from, and whether it can be saved back.
    bundle: Option<(String, bool)>,
    modified: bool,
    swf_ptr: &'static Swf,
    exporter: ManuallyDrop<Exporter<'static>>,
}

impl Doc {
    fn new(id: u32, path: PathBuf, swf: Swf) -> Doc {
        let swf_ptr: &'static Swf = Box::leak(Box::new(swf));
        Doc { id, path, bundle: None, modified: false, swf_ptr, exporter: ManuallyDrop::new(Exporter::new(swf_ptr)) }
    }

    /// Applies `f` to a copy of the file, which replaces it only when `f` succeeds.
    fn edit(&mut self, f: impl FnOnce(&mut Swf) -> Res<()>) -> Res<()> {
        let mut swf = self.swf().clone();
        f(&mut swf)?;
        let mut doc = Doc::new(self.id, self.path.clone(), swf);
        doc.bundle = self.bundle.take();
        doc.modified = true;
        *self = doc;
        Ok(())
    }

    fn name(&self) -> String {
        let file = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match &self.bundle {
            Some((key, _)) => format!("{file}/{key}"),
            None => file,
        }
    }

    fn save_to(&mut self, path: PathBuf, opts: &SaveOptions) -> Res<()> {
        let bytes = self.swf().save(opts).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.path = path;
        self.bundle = None;
        self.modified = false;
        Ok(())
    }

    /// Writes the SWF back where it came from, inside its archive for a bundle entry.
    fn save(&mut self) -> Res<()> {
        let opts = SaveOptions { compression: self.swf().header.compression, encrypt: self.swf().header.encrypted };
        let Some((key, writable)) = self.bundle.clone() else { return self.save_to(self.path.clone(), &opts) };
        if !writable {
            return Err("this SWF was found inside another file; use Save as".into());
        }
        let bytes = self.swf().save(&opts).map_err(|e| e.to_string())?;
        let err = |e: &dyn std::fmt::Display| format!("{}: {e}", self.path.display());
        let archive = std::fs::read(&self.path).map_err(|e| err(&e))?;
        let out = ffdec::bundle::replace(&archive, &key, &bytes).map_err(|e| err(&e))?;
        std::fs::write(&self.path, out).map_err(|e| err(&e))?;
        self.modified = false;
        Ok(())
    }

    fn swf(&self) -> &Swf {
        self.swf_ptr
    }

    fn exporter(&self) -> &Exporter<'_> {
        &self.exporter
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        // SAFETY: the exporter is the only holder of `swf_ptr` beyond `self`, and neither is
        // handed out past a `&self` borrow, so freeing the exporter first leaves no users.
        unsafe {
            ManuallyDrop::drop(&mut self.exporter);
            drop(Box::from_raw(self.swf_ptr as *const Swf as *mut Swf));
        }
    }
}

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
    importable: bool,
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

fn tag_path(path: &str) -> Res<Vec<usize>> {
    if path.is_empty() {
        Ok(Vec::new())
    } else {
        tree::parse_path(path).ok_or_else(|| "bad tag path".to_string())
    }
}

fn find_tag<'a>(swf: &'a Swf, path: &str) -> Res<&'a ffdec::Tag> {
    let p = tree::parse_path(path).ok_or("bad tag path")?;
    swf.tag(&p).ok_or_else(|| "no such tag".to_string())
}

fn add(docs: &Docs, path: PathBuf, bundle: Option<(String, bool)>, swf: Swf) -> Opened {
    let id = docs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let mut doc = Doc::new(id, path, swf);
    doc.bundle = bundle;
    let opened = Opened { id, name: doc.name(), path: doc.path.display().to_string(), warnings: doc.swf().warnings.clone() };
    docs.open.lock().push(doc);
    opened
}

/// Opens a SWF, or every SWF in a ZIP, SWC or other file.
fn load(docs: &Docs, path: PathBuf) -> Res<Vec<Opened>> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let data = std::fs::read(&path).map_err(|e| err(&e))?;
    if !ffdec::bundle::is_bundle(&path) || Swf::parse(&data).is_ok() {
        let swf = Swf::parse(&data).map_err(|e| err(&e))?;
        return Ok(vec![add(docs, path, None, swf)]);
    }
    let b = ffdec::bundle::open(&path, &data).map_err(|e| err(&e))?;
    let writable = !b.read_only();
    let mut out = Vec::new();
    for (key, bytes) in b.entries {
        match Swf::parse(&bytes) {
            Ok(swf) => out.push(add(docs, path.clone(), Some((key, writable)), swf)),
            Err(e) => return Err(format!("{}/{key}: {e}", path.display())),
        }
    }
    if out.is_empty() {
        return Err(err(&"no SWF files found"));
    }
    Ok(out)
}

#[tauri::command]
async fn open_dialog(app: tauri::AppHandle, docs: State<'_, Docs>) -> Res<Vec<Opened>> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Flash files", &["swf", "gfx", "spl", "swc", "zip"])
        .add_filter("All files", &["*"])
        .blocking_pick_files()
        .unwrap_or_default();
    let mut out = Vec::new();
    for p in picked {
        out.extend(load(&docs, p.into_path().map_err(|e| e.to_string())?)?);
    }
    Ok(out)
}

#[tauri::command]
fn open_path(docs: State<'_, Docs>, path: String) -> Res<Vec<Opened>> {
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
        if let Some((key, _)) = &d.bundle {
            out.insert(1, f("Entry", key.clone()));
        }
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
        Ok(TagInfo { fields: describe(t), size: t.body.len(), importable: ffdec::importers::Asset::of(t.code).is_some() })
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
fn save(docs: State<'_, Docs>, id: u32) -> Res<String> {
    with_doc_mut(&docs, id, |d| {
        d.save()?;
        Ok(d.name())
    })
}

/// Saves under a new name, which the document then takes.
#[tauri::command]
async fn save_as(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, compression: String, encrypt: bool) -> Res<Option<Opened>> {
    let compression = Compression::parse(&compression).ok_or("unknown compression")?;
    let name = with_doc(&docs, id, |d| Ok(d.name().rsplit('/').next().unwrap_or_default().trim_matches(['[', ']']).to_string()))?;
    let Some(out) = save_dialog(&app, &name, ("SWF", &["swf"])) else { return Ok(None) };
    with_doc_mut(&docs, id, |d| {
        d.save_to(out, &SaveOptions { compression, encrypt })?;
        Ok(Some(Opened { id, name: d.name(), path: d.path.display().to_string(), warnings: Vec::new() }))
    })
}

/// Replaces an image, sound, binary data or edit text with a file the user picks.
#[tauri::command]
async fn replace_asset(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, path: String) -> Res<Option<String>> {
    let p = tag_path(&path)?;
    let asset = with_doc(&docs, id, |d| {
        let t = d.swf().tag(&p).ok_or("no such tag")?;
        ffdec::importers::Asset::of(t.code).ok_or_else(|| format!("{} tags cannot be replaced yet", t.name()))
    })?;
    let mut dialog = app.dialog().file();
    if !asset.extensions().is_empty() {
        dialog = dialog.add_filter(format!("{asset:?} files"), asset.extensions());
    }
    let Some(file) = dialog.add_filter("All files", &["*"]).blocking_pick_file().and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    with_doc_mut(&docs, id, |d| d.edit(|swf| ffdec::importers::import(swf, &p, &data).map_err(|e| e.to_string())))?;
    Ok(Some(file.display().to_string()))
}

/// Adds characters to a DefineFont2/3 from a font file the user picks, also redrawing the
/// characters it has; returns how many glyphs were set.
#[tauri::command]
async fn add_font_chars(app: tauri::AppHandle, docs: State<'_, Docs>, id: u32, path: String, chars: String) -> Res<Option<usize>> {
    let p = tag_path(&path)?;
    let dialog = app.dialog().file().add_filter("Fonts", &["ttf", "otf"]).add_filter("All files", &["*"]);
    let Some(file) = dialog.blocking_pick_file().and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let chars: Vec<char> = chars.chars().filter(|c| !c.is_control()).collect();
    let mut n = 0;
    with_doc_mut(&docs, id, |d| d.edit(|swf| ffdec::importers::font(swf, &p, &data, &chars).map(|c| n = c).map_err(|e| e.to_string())))?;
    Ok(Some(n))
}

#[tauri::command]
fn get_header(docs: State<'_, Docs>, id: u32) -> Res<ffdec::edit::HeaderEdit> {
    with_doc(&docs, id, |d| Ok(ffdec::edit::header(d.swf())))
}

#[tauri::command]
fn set_header(docs: State<'_, Docs>, id: u32, header: ffdec::edit::HeaderEdit) -> Res<()> {
    with_doc_mut(&docs, id, |d| d.edit(|swf| ffdec::edit::set_header(swf, &header)))
}

#[tauri::command]
fn remove_tag(docs: State<'_, Docs>, id: u32, path: String) -> Res<()> {
    let p = tag_path(&path)?;
    with_doc_mut(&docs, id, |d| d.edit(|swf| swf.remove_tag(&p).map(|_| ()).map_err(|e| e.to_string())))
}

#[derive(Serialize)]
struct EditError {
    line: usize,
    message: String,
}

impl From<ffdec::action::asm::ParseError> for EditError {
    fn from(e: ffdec::action::asm::ParseError) -> Self {
        EditError { line: e.line, message: e.message }
    }
}

impl From<String> for EditError {
    fn from(message: String) -> Self {
        EditError { line: 0, message }
    }
}

/// Compiles edited ActionScript into the script.
#[tauri::command]
async fn apply_source(docs: State<'_, Docs>, id: u32, path: String, script: String, text: String) -> Result<(), EditError> {
    let p = tag_path(&path)?;
    let mut failure = None;
    let r = with_doc_mut(&docs, id, |d| {
        d.edit(|swf| {
            ffdec::edit::set_script_source(swf, &p, &script, &text).map_err(|e| {
                let m = e.message.clone();
                failure = Some(e);
                m
            })
        })
    });
    r.map_err(|m| failure.map_or_else(|| EditError::from(m), EditError::from))
}

#[tauri::command]
fn script_methods(docs: State<'_, Docs>, id: u32, path: String, script: String) -> Res<Vec<String>> {
    let p = tag_path(&path)?;
    with_doc(&docs, id, |d| ffdec::edit::script_methods(d.swf(), &p, &script).map_err(|e| e.message))
}

#[tauri::command]
fn script_pcode(docs: State<'_, Docs>, id: u32, path: String, script: String, method: Option<String>) -> Res<String> {
    let p = tag_path(&path)?;
    with_doc(&docs, id, |d| ffdec::edit::script_pcode(d.swf(), &p, &script, method.as_deref()).map_err(|e| e.message))
}

/// Assembles edited p-code into the script; errors carry the line they are on.
#[tauri::command]
async fn apply_pcode(docs: State<'_, Docs>, id: u32, path: String, script: String, method: Option<String>, text: String) -> Result<(), EditError> {
    let p = tag_path(&path)?;
    let mut failure = None;
    let r = with_doc_mut(&docs, id, |d| {
        d.edit(|swf| {
            ffdec::edit::set_script_pcode(swf, &p, &script, method.as_deref(), &text).map_err(|e| {
                let m = e.message.clone();
                failure = Some(e);
                m
            })
        })
    });
    r.map_err(|m| failure.map_or_else(|| EditError::from(m), EditError::from))
}

#[tauri::command]
fn text_source(docs: State<'_, Docs>, id: u32, path: String) -> Res<Option<String>> {
    let p = tag_path(&path)?;
    with_doc(&docs, id, |d| ffdec::edit::text_source(d.swf(), &p).map_err(|e| e.message))
}

/// Stores an edited text; errors carry the line they are on.
#[tauri::command]
async fn apply_text(docs: State<'_, Docs>, id: u32, path: String, text: String) -> Result<(), EditError> {
    let p = tag_path(&path)?;
    let mut failure = None;
    let r = with_doc_mut(&docs, id, |d| {
        d.edit(|swf| {
            ffdec::edit::set_text_source(swf, &p, &text).map_err(|e| {
                let m = e.message.clone();
                failure = Some(e);
                m
            })
        })
    });
    r.map_err(|m| failure.map_or_else(|| EditError::from(m), EditError::from))
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
    let p = tag_path(&path)?;
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

fn discard_dialog(app: &tauri::AppHandle, names: &str) -> tauri_plugin_dialog::MessageDialogBuilder<tauri::Wry> {
    app.dialog()
        .message(format!("Discard unsaved changes to {names}?"))
        .title("Rust-FFDec")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Discard".into(), "Cancel".into()))
}

/// Asks before closing the window while a file has unsaved changes.
fn guard_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    let tauri::WindowEvent::CloseRequested { api, .. } = event else { return };
    let app = window.app_handle();
    let unsaved: Vec<String> = app.state::<Docs>().open.lock().iter().filter(|d| d.modified).map(Doc::name).collect();
    if unsaved.is_empty() {
        return;
    }
    api.prevent_close();
    let window = window.clone();
    discard_dialog(app, &unsaved.join(", ")).show(move |discard| {
        if discard {
            window.app_handle().state::<Docs>().open.lock().clear();
            let _ = window.destroy();
        }
    });
}

/// Asks before discarding a file's unsaved changes.
#[tauri::command]
async fn confirm_discard(app: tauri::AppHandle, name: String) -> bool {
    discard_dialog(&app, &name).blocking_show()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Docs::default())
        .on_window_event(guard_close)
        .invoke_handler(tauri::generate_handler![
            open_dialog,
            open_path,
            close_doc,
            get_tree,
            header_info,
            tag_info,
            tag_bytes,
            export_tag,
            save,
            save_as,
            replace_asset,
            script_methods,
            script_pcode,
            apply_pcode,
            text_source,
            get_header,
            set_header,
            remove_tag,
            add_font_chars,
            apply_text,
            apply_source,
            confirm_discard,
            preview_info,
            preview_data,
            export_asset,
            export_all,
            startup_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rust-FFDec");
}
