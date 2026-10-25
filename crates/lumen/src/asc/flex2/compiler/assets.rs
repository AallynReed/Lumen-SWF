//! Ports of `flex2.compiler.Assets` and `flex2.compiler.AssetInfo`.

use super::source::VirtualFile;
use super::transcoder::Args;
use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::util::jhash::JHashMap;

/// `AssetInfo`.
#[derive(Clone)]
pub struct AssetInfo {
    pub define_tag: TagRef,
    pub path: Option<VirtualFile>,
    pub creation_time: i64,
    pub args: Option<Args>,
}

impl AssetInfo {
    pub fn new(define_tag: TagRef, path: Option<VirtualFile>, creation_time: i64, args: Option<Args>) -> AssetInfo {
        AssetInfo { define_tag, path, creation_time, args }
    }

    /// `AssetInfo(defineTag)`, as `CompilerSwcContext` adds a library's symbols.
    pub fn from_tag(define_tag: TagRef) -> AssetInfo {
        AssetInfo { define_tag, path: None, creation_time: 0, args: None }
    }

    pub fn get_define_tag(&self) -> &TagRef {
        &self.define_tag
    }
}

/// `Assets`: a `HashMap<String, AssetInfo>(4)` made on first add.
#[derive(Clone, Default)]
pub struct Assets {
    assets: Option<JHashMap<String, AssetInfo>>,
}

impl Assets {
    pub fn add(&mut self, class_name: &str, info: AssetInfo) {
        self.assets.get_or_insert_with(|| JHashMap::with_capacity(4)).insert(class_name.to_string(), info);
    }

    pub fn add_all(&mut self, other: &Assets) {
        let Some(o) = &other.assets else { return };
        let m = self.assets.get_or_insert_with(|| JHashMap::with_capacity(4));
        m.put_all(o.iter().map(|(k, v)| (k.clone(), v.clone())), o.len());
    }

    pub fn count(&self) -> usize {
        self.assets.as_ref().map_or(0, |a| a.len())
    }

    pub fn contains(&self, class_name: &str) -> bool {
        self.assets.as_ref().is_some_and(|a| a.contains_key(class_name))
    }

    pub fn get(&self, class_name: &str) -> Option<&AssetInfo> {
        self.assets.as_ref().and_then(|a| a.get(class_name))
    }

    /// `iterator()`, in `HashMap` order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &AssetInfo)> {
        self.assets.iter().flat_map(|a| a.iter())
    }

    /// `getFonts()`.
    pub fn get_fonts(&self) -> Vec<TagRef> {
        self.iter().filter(|(_, a)| a.define_tag.borrow().is_define_font()).map(|(_, a)| a.define_tag.clone()).collect()
    }

    pub fn size(&self) -> usize {
        self.count()
    }
}
