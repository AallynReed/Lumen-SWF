//! FFDec-style resource tree (shapes, sprites, images, …) and the raw tag list.

use std::collections::HashMap;

use serde::Serialize;

use crate::info::character_names;
use crate::tags::{code::*, Tag, Timeline};
use crate::Swf;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub label: String,
    /// `header`, `folder`, `frame` or `tag`.
    pub kind: &'static str,
    /// Folder or tag category, used for icons.
    pub category: &'static str,
    /// Tag path such as `"12/3"`; set for tag nodes.
    pub path: Option<String>,
    pub children: Vec<Node>,
}

impl Node {
    fn folder(label: &str, category: &'static str, children: Vec<Node>) -> Node {
        Node { label: label.into(), kind: "folder", category, path: None, children }
    }
}

pub fn category(code: u16) -> &'static str {
    match code {
        DEFINE_SHAPE | DEFINE_SHAPE2 | DEFINE_SHAPE3 | DEFINE_SHAPE4 => "shapes",
        DEFINE_MORPH_SHAPE | DEFINE_MORPH_SHAPE2 => "morphshapes",
        DEFINE_SPRITE => "sprites",
        DEFINE_TEXT | DEFINE_TEXT2 | DEFINE_EDIT_TEXT => "texts",
        DEFINE_BITS | DEFINE_BITS_JPEG2 | DEFINE_BITS_JPEG3 | DEFINE_BITS_JPEG4 | DEFINE_BITS_LOSSLESS
        | DEFINE_BITS_LOSSLESS2 | GFX_DEFINE_EXTERNAL_IMAGE | GFX_DEFINE_EXTERNAL_IMAGE2 | GFX_DEFINE_SUB_IMAGE => "images",
        DEFINE_VIDEO_STREAM => "movies",
        DEFINE_SOUND | SOUND_STREAM_HEAD | SOUND_STREAM_HEAD2 | GFX_DEFINE_EXTERNAL_SOUND
        | GFX_DEFINE_EXTERNAL_STREAM_SOUND => "sounds",
        DEFINE_BUTTON | DEFINE_BUTTON2 => "buttons",
        DEFINE_FONT | DEFINE_FONT2 | DEFINE_FONT3 | DEFINE_FONT4 | GFX_DEFINE_COMPACTED_FONT => "fonts",
        DEFINE_BINARY_DATA => "binaryData",
        DO_ABC | DO_ABC2 | DO_ACTION | DO_INIT_ACTION => "scripts",
        _ if is_frame_tag(code) => "frames",
        _ => "others",
    }
}

fn is_frame_tag(code: u16) -> bool {
    matches!(
        code,
        END | SHOW_FRAME
            | PLACE_OBJECT
            | PLACE_OBJECT2
            | PLACE_OBJECT3
            | PLACE_OBJECT4
            | REMOVE_OBJECT
            | REMOVE_OBJECT2
            | START_SOUND
            | START_SOUND2
            | FRAME_LABEL
            | SOUND_STREAM_BLOCK
            | VIDEO_FRAME
            | SET_TAB_INDEX
    )
}

/// Reference tags shown beneath the character they describe.
fn attaches_to_character(code: u16) -> bool {
    matches!(
        code,
        DEFINE_FONT_INFO
            | DEFINE_FONT_INFO2
            | DEFINE_FONT_ALIGN_ZONES
            | DEFINE_FONT_NAME
            | CSM_SETTINGS
            | DEFINE_SCALING_GRID
            | DEFINE_BUTTON_SOUND
            | DEFINE_BUTTON_CXFORM
            | VIDEO_FRAME
    )
}

fn path_str(path: &[usize]) -> String {
    path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("/")
}

pub fn tag_label(tag: &Tag, names: &HashMap<u16, String>) -> String {
    let name = tag.name();
    match tag.character_id() {
        Some(id) if crate::tags::defines_character(tag.code) => match names.get(&id) {
            Some(n) => format!("{name} ({id}: {n})"),
            None => format!("{name} ({id})"),
        },
        Some(id) => format!("{name} ({id})"),
        None => name,
    }
}

fn tag_node(tag: &Tag, path: &[usize], names: &HashMap<u16, String>) -> Node {
    Node { label: tag_label(tag, names), kind: "tag", category: category(tag.code), path: Some(path_str(path)), children: vec![] }
}

fn frames(tl: &Timeline, base: &[usize], names: &HashMap<u16, String>, only_frame_tags: bool) -> Vec<Node> {
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut n = 1;
    for (i, t) in tl.tags.iter().enumerate() {
        if t.code == END {
            continue;
        }
        let mut p = base.to_vec();
        p.push(i);
        if t.code == SHOW_FRAME {
            out.push(Node { label: format!("frame {n}"), kind: "frame", category: "frames", path: Some(path_str(&p)), children: std::mem::take(&mut current) });
            n += 1;
        } else if !only_frame_tags || is_frame_tag(t.code) || matches!(t.code, DO_ACTION) {
            current.push(tag_node(t, &p, names));
        }
    }
    if !current.is_empty() {
        out.push(Node { label: format!("frame {n}"), kind: "frame", category: "frames", path: None, children: current });
    }
    out
}

pub fn resource_tree(swf: &Swf) -> Vec<Node> {
    let names = character_names(swf);
    let order = ["shapes", "morphshapes", "sprites", "texts", "images", "movies", "sounds", "buttons", "fonts", "binaryData"];
    let mut folders: HashMap<&str, Vec<Node>> = HashMap::new();
    let mut others = Vec::new();
    let mut scripts = Vec::new();
    let mut by_char: HashMap<u16, (&str, usize)> = HashMap::new();
    let mut attached: Vec<(u16, Node)> = Vec::new();

    for (i, t) in swf.timeline.tags.iter().enumerate() {
        let path = [i];
        let cat = category(t.code);
        if attaches_to_character(t.code) {
            if let Some(id) = t.character_id() {
                attached.push((id, tag_node(t, &path, &names)));
                continue;
            }
        }
        match cat {
            "frames" => {}
            "scripts" => scripts.push(script_node(t, &path, &names, swf)),
            "others" => others.push(tag_node(t, &path, &names)),
            _ => {
                let mut node = tag_node(t, &path, &names);
                if t.code == DEFINE_SPRITE {
                    if let Some(s) = &t.sprite {
                        node.children = frames(s, &path, &names, false);
                    }
                }
                let list = folders.entry(cat).or_default();
                if let Some(id) = t.character_id() {
                    by_char.insert(id, (cat, list.len()));
                }
                list.push(node);
            }
        }
    }
    for (id, node) in attached {
        match by_char.get(&id) {
            Some(&(cat, idx)) => folders.get_mut(cat).unwrap()[idx].children.push(node),
            None => others.push(node),
        }
    }

    let mut root = vec![Node { label: "header".into(), kind: "header", category: "header", path: None, children: vec![] }];
    for cat in order {
        if let Some(list) = folders.remove(cat) {
            root.push(Node::folder(cat, cat, list));
        }
    }
    let root_frames = frames(&swf.timeline, &[], &names, true);
    if !root_frames.is_empty() {
        root.push(Node::folder("frames", "frames", root_frames));
    }
    root.push(Node::folder("others", "others", others));
    if !scripts.is_empty() {
        root.push(Node::folder("scripts", "scripts", scripts));
    }
    root
}

fn script_node(t: &Tag, path: &[usize], names: &HashMap<u16, String>, swf: &Swf) -> Node {
    let mut node = tag_node(t, path, names);
    match t.code {
        DO_ABC2 => {
            let mut r = crate::io::Reader::new(&t.body);
            if r.skip(4).is_ok() {
                if let Ok(n) = r.string() {
                    if !n.is_empty() {
                        node.label = format!("DoABC2 ({n})");
                    }
                }
            }
        }
        DO_ACTION => {
            let frame = swf.timeline.tags[..path[0]].iter().filter(|x| x.code == SHOW_FRAME).count() + 1;
            node.label = format!("frame {frame} (DoAction)");
        }
        DO_INIT_ACTION => {
            if let Some(id) = t.character_id() {
                node.label = match names.get(&id) {
                    Some(n) => format!("{n} (DoInitAction {id})"),
                    None => format!("sprite {id} (DoInitAction)"),
                };
            }
        }
        _ => {}
    }
    node
}

pub fn tag_list(swf: &Swf) -> Vec<Node> {
    let names = character_names(swf);
    fn go(tl: &Timeline, base: &mut Vec<usize>, names: &HashMap<u16, String>) -> Vec<Node> {
        tl.tags
            .iter()
            .enumerate()
            .map(|(i, t)| {
                base.push(i);
                let mut node = tag_node(t, base, names);
                if let Some(s) = &t.sprite {
                    node.children = go(s, base, names);
                }
                base.pop();
                node
            })
            .collect()
    }
    let mut root = vec![Node { label: "header".into(), kind: "header", category: "header", path: None, children: vec![] }];
    root.extend(go(&swf.timeline, &mut Vec::new(), &names));
    root
}

pub fn parse_path(s: &str) -> Option<Vec<usize>> {
    s.split('/').map(|p| p.parse().ok()).collect()
}
