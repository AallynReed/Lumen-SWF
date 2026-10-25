//! Port of `flash.swf.Dictionary`, and the `HashSet<Tag>` `MovieEncoder` keeps: both find tags by `equals`, so equal
//! definitions share one id.

use std::collections::HashMap;
use std::rc::Rc;

use super::tags::{tag_eq, TagRef};

/// Equal tags always share `code` and `name`, so those pick the bucket.
fn bucket_key(tag: &TagRef) -> (i32, Option<String>) {
    let t = tag.borrow();
    (t.code, t.name.clone())
}

/// A `HashSet<Tag>` / `HashMap<Tag, V>` by Java `equals`.
pub struct TagMap<V> {
    buckets: HashMap<(i32, Option<String>), Vec<(TagRef, V)>>,
}

impl<V: Clone> Default for TagMap<V> {
    fn default() -> Self {
        TagMap { buckets: HashMap::new() }
    }
}

/// `DefineShape` and `DefineText` hash their bounds `Rect`, whose `hashCode` starts from `Object.hashCode`, so a
/// hash lookup only ever finds the same object.
fn identity_hashed(tag: &TagRef) -> bool {
    matches!(tag.borrow().body, super::tags::Body::DefineShape(_) | super::tags::Body::DefineText(_))
}

impl<V: Clone> TagMap<V> {
    pub fn get(&self, tag: &TagRef) -> Option<V> {
        let b = self.buckets.get(&bucket_key(tag))?;
        if identity_hashed(tag) {
            return b.iter().find(|(t, _)| Rc::ptr_eq(t, tag)).map(|(_, v)| v.clone());
        }
        b.iter().find(|(t, _)| tag_eq(t, tag)).map(|(_, v)| v.clone())
    }

    pub fn contains(&self, tag: &TagRef) -> bool {
        self.get(tag).is_some()
    }

    pub fn put(&mut self, tag: &TagRef, v: V) {
        let identity = identity_hashed(tag);
        let b = self.buckets.entry(bucket_key(tag)).or_default();
        match b.iter_mut().find(|(t, _)| if identity { Rc::ptr_eq(t, tag) } else { tag_eq(t, tag) }) {
            Some(e) => e.1 = v,
            None => b.push((tag.clone(), v)),
        }
    }
}

#[derive(Default)]
pub struct Dictionary {
    ids: HashMap<i32, TagRef>,
    tags: TagMap<i32>,
    names: HashMap<String, TagRef>,
    fonts: HashMap<String, TagRef>,
    next_id: i32,
}

impl Dictionary {
    pub fn new() -> Dictionary {
        Dictionary { next_id: 1, ..Default::default() }
    }

    pub fn contains_id(&self, id: i32) -> bool {
        self.ids.contains_key(&id)
    }

    /// `getId(tag)`: by `equals` when encoding, else the id it was read with.
    pub fn get_id(&self, tag: Option<&TagRef>) -> i32 {
        let Some(tag) = tag else { return -1 };
        if let Some(id) = self.tags.get(tag) {
            return id;
        }
        self.ids.iter().find(|(_, t)| Rc::ptr_eq(t, tag)).map(|(id, _)| *id).unwrap_or(-1)
    }

    /// `add(tag)`, the encoding side.
    pub fn add(&mut self, tag: &TagRef) -> i32 {
        if let Some(id) = self.tags.get(tag) {
            return id;
        }
        let key = self.next_id;
        self.next_id += 1;
        self.tags.put(tag, key);
        self.ids.insert(key, tag.clone());
        key
    }

    /// `add(id, tag)`, the decoding side.
    pub fn add_with_id(&mut self, id: i32, tag: TagRef) -> Result<(), String> {
        match self.ids.get(&id) {
            None => {
                self.ids.insert(id, tag);
                Ok(())
            }
            Some(t) => {
                if tag_eq(t, &tag) {
                    Err(format!("symbol {id} redefined by identical tag"))
                } else {
                    Err(format!("symbol {id} redefined by different tag"))
                }
            }
        }
    }

    pub fn add_name(&mut self, tag: &TagRef, name: &str) {
        self.names.insert(name.to_string(), tag.clone());
    }

    fn make_font_key(name: &str, bold: bool, italic: bool) -> String {
        format!("{}{}{}", name, if bold { "_bold_" } else { "_normal_" }, if italic { "_italic" } else { "_regular" })
    }

    pub fn add_font_face(&mut self, font: &TagRef) {
        let key = {
            let f = font.borrow();
            Self::make_font_key(&f.get_font_name().unwrap_or_else(|| "null".to_string()), f.is_bold(), f.is_italic())
        };
        self.fonts.insert(key, font.clone());
    }

    pub fn get_font_face(&self, name: &str, bold: bool, italic: bool) -> Option<TagRef> {
        self.fonts.get(&Self::make_font_key(name, bold, italic)).cloned()
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub fn get_tag_by_name(&self, name: &str) -> Option<TagRef> {
        self.names.get(name).cloned()
    }

    /// `getTag(idref)`; `Ok(None)` is the `INVALID_TAG` for the authoring tools' bogus 65535 ids.
    pub fn get_tag(&self, idref: i32) -> Result<Option<TagRef>, String> {
        match self.ids.get(&idref) {
            Some(t) => Ok(Some(t.clone())),
            None if idref == 65535 => Ok(None),
            None => Err(format!("symbol {idref} not defined")),
        }
    }
}
