#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use ffdec::info::{describe, Field};
use ffdec::tree::{self, Node};
use ffdec::{Compression, SaveOptions, Swf};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

struct Doc {
    id: u32,
    path: PathBuf,
    swf: Swf,
}

#[derive(Default)]
struct Docs {
    open: Mutex<Vec<Doc>>,
    next_id: AtomicU32,
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
    docs.open.lock().push(Doc { id, path, swf });
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
    with_doc(&docs, id, |d| Ok(if raw { tree::tag_list(&d.swf) } else { tree::resource_tree(&d.swf) }))
}

#[tauri::command]
fn header_info(docs: State<'_, Docs>, id: u32) -> Res<Vec<Field>> {
    with_doc(&docs, id, |d| {
        let s = &d.swf;
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
        let t = find_tag(&d.swf, &path)?;
        Ok(TagInfo { fields: describe(t), size: t.body.len() })
    })
}

#[tauri::command]
fn tag_bytes(docs: State<'_, Docs>, id: u32, path: String, offset: usize, len: usize) -> Res<Bytes> {
    with_doc(&docs, id, |d| {
        let t = find_tag(&d.swf, &path)?;
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
        let t = find_tag(&d.swf, &path)?;
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
        let bytes = d.swf.save(&SaveOptions { compression, encrypt }).map_err(|e| e.to_string())?;
        Ok((bytes, d.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
    })?;
    let Some(out) = save_dialog(&app, &name, ("SWF", &["swf"])) else { return Ok(None) };
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(Some(out.display().to_string()))
}

#[tauri::command]
fn startup_files() -> Vec<String> {
    std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect()
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
            startup_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rust-FFDec");
}
