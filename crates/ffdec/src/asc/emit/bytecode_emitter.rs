//! Port of flex2's `flex2/compiler/as3/BytecodeEmitter.java`, the `ActionBlockEmitter` mxmlc uses: it drops
//! `[Embed]` metadata and maps debug line numbers of generated code back to the original MXML.
//! `Source` and `LineNumberMap` are Flex infrastructure; [`Source`] and [`LineNumberMap`] stub only what it reads.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR};
use std::rc::Rc;

use super::action_block_emitter::{ActionBlockEmitter, ActionBlockEmitterHooks};
use crate::asc::semantics::meta_data::MetaData;
use crate::asc::util::context::ContextRef;
use crate::asc::util::int_list::IntList;

/// `StandardDefs.MD_EMBED`.
pub const MD_EMBED: &str = "Embed";

/// The parts of `flex2.compiler.Source` that `BytecodeEmitter` uses.
#[derive(Default)]
pub struct Source {
    pub name: String,
    pub debuggable: bool,
    pub source_path_owner: bool,
    pub source_list_owner: bool,
    pub resource_container_owner: bool,
    pub root: bool,
    pub relative_path: String,
    /// `Source.lineCount`.
    pub line_count: Cell<i32>,
}

/// The parts of `flex2.compiler.util.LineNumberMap` that `BytecodeEmitter` uses.
#[derive(Default)]
pub struct LineNumberMap {
    pub old_name: String,
    pub new_name: String,
    /// New line to old line; a missing line maps to 0.
    pub lines: HashMap<i32, i32>,
}

impl LineNumberMap {
    pub fn get(&self, new_line: i32) -> i32 {
        self.lines.get(&new_line).copied().unwrap_or(0)
    }
}

/// The `BytecodeEmitter` fields; installed on its `ActionBlockEmitter` as the overrides.
pub struct BytecodeEmitter {
    map: Option<Rc<LineNumberMap>>,
    source: Option<Rc<Source>>,
    current_file_name: Option<String>,
    keep_embed: bool,
    lines: Option<HashSet<(Option<String>, i32)>>,
}

/// `new BytecodeEmitter(cx, source, debug, codeHints)`.
pub fn new_bytecode_emitter(cx: &ContextRef, source: Option<Rc<Source>>, debug: bool, code_hints: bool) -> ActionBlockEmitter {
    new_bytecode_emitter_full(cx, source, debug, code_hints, false, None)
}

/// `new BytecodeEmitter(cx, source, debug, codeHints, keepEmbed, map)`.
pub fn new_bytecode_emitter_full(
    cx: &ContextRef,
    source: Option<Rc<Source>>,
    debug: bool,
    code_hints: bool,
    keep_embed: bool,
    map: Option<Rc<LineNumberMap>>,
) -> ActionBlockEmitter {
    let scriptname = source.as_ref().map(|s| s.name.clone());
    let mut abe = ActionBlockEmitter::new(cx, scriptname.as_deref(), false, false, false, debug, code_hints);
    let be = BytecodeEmitter { map, source, current_file_name: None, keep_embed, lines: if debug { Some(HashSet::new()) } else { None } };
    abe.set_hooks(Some(Box::new(be)));
    abe
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_utf16(s: &[u16]) -> String {
    String::from_utf16_lossy(s)
}

/// `String.lastIndexOf(str, fromIndex)` in UTF-16 units.
fn last_index_of_from(s: &[u16], pat: &[u16], from: i32) -> i32 {
    let max_start = s.len() as i32 - pat.len() as i32;
    let mut i = from.min(max_start);
    while i >= 0 {
        if &s[i as usize..i as usize + pat.len()] == pat {
            return i;
        }
        i -= 1;
    }
    -1
}

impl BytecodeEmitter {
    fn source(&self) -> &Source {
        self.source.as_deref().expect("NullPointerException")
    }

    fn calculate_line_number(&self, line_number: i32) -> i32 {
        match &self.map {
            Some(map) if self.current_file_name.as_deref() == Some(self.source().name.as_str()) => {
                let new_line_number = map.get(line_number);
                if new_line_number > 0 {
                    new_line_number
                } else {
                    -1
                }
            }
            _ => line_number,
        }
    }
}

impl ActionBlockEmitterHooks for BytecodeEmitter {
    fn add_metadata(&mut self, abe: &mut ActionBlockEmitter, metadata: Option<&[MetaData]>) -> Option<IntList> {
        if self.keep_embed {
            return abe.base_add_metadata(metadata);
        }

        let mut meta_data_indices = None;
        if let Some(metadata) = metadata.filter(|m| !m.is_empty()) {
            let mut indices = Vec::with_capacity(metadata.len());
            for meta_data in metadata {
                let id = meta_data.id();
                if id.as_deref().expect("NullPointerException") != MD_EMBED {
                    let values = meta_data.values();
                    indices.push(abe.add_metadata_info(id.as_deref(), values.as_deref()));
                }
            }
            meta_data_indices = Some(indices);
        }
        meta_data_indices
    }

    fn debug_slot(&mut self, abe: &mut ActionBlockEmitter, name: &str, slot: i32, line: i32) {
        if self.source().debuggable {
            let new_line = self.calculate_line_number(line);
            if new_line != -1 {
                abe.base_debug_slot(name, slot, new_line);
            }
        }
    }

    fn debug_file(&mut self, abe: &mut ActionBlockEmitter, name: &str) {
        self.current_file_name = Some(name.to_string());

        if !self.source().debuggable {
            return;
        }

        let mut name = name.to_string();
        if let Some(map) = &self.map {
            if map.new_name == name {
                name = map.old_name.clone();
            }
        }

        let source = self.source();
        if source.source_path_owner || source.source_list_owner || source.resource_container_owner || source.root {
            let relative_path = source.relative_path.replace('/', MAIN_SEPARATOR_STR);
            let n = utf16(&name);
            let sep = MAIN_SEPARATOR as u16;
            if relative_path.is_empty() {
                if let Some(index) = n.iter().rposition(|&c| c == sep) {
                    name = format!("{};;{}", from_utf16(&n[..index]), from_utf16(&n[index + 1..]));
                }
            } else {
                let rp = utf16(&relative_path);
                let separator_index = n.iter().rposition(|&c| c == sep).map_or(-1, |i| i as i32);
                let index = if separator_index > -1 {
                    last_index_of_from(&n, &rp, separator_index)
                } else {
                    last_index_of_from(&n, &rp, n.len() as i32)
                };
                if index > 0 {
                    let i = index as usize;
                    name = format!("{};{};{}", from_utf16(&n[..i - 1]), relative_path, from_utf16(&n[i + rp.len() + 1..]));
                }
            }
        }

        abe.base_debug_file(&name);
    }

    fn debug_line_op(&mut self, abe: &mut ActionBlockEmitter, line_number: i32) {
        if !self.source().debuggable {
            return;
        }

        if let Some(lines) = &mut self.lines {
            let key = (self.current_file_name.clone(), line_number);
            if !lines.contains(&key) {
                lines.insert(key);
                self.source.as_ref().unwrap().line_count.set(lines.len() as i32);
            }
        }

        let new_line_number = self.calculate_line_number(line_number);

        if new_line_number > 0 {
            abe.base_debug_line_op(new_line_number);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Mirrored by a Java driver running the real `BytecodeEmitter` over stubbed `Source`/`LineNumberMap`.

    use std::cell::RefCell;

    use super::*;
    use crate::asc::emit::features::{DIALECT_AS3, TARGET_AVM2};
    use crate::asc::emit::global_builder::new_global_builder;
    use crate::asc::emit::runtime_constants::TYPE_void;
    use crate::asc::semantics::emitter::{Emitter, EmitterRef};
    use crate::asc::semantics::object_value::ObjectValue;
    use crate::asc::semantics::type_value::TypeValue;
    use crate::asc::semantics::value::{KeyValuePair, Value};
    use crate::asc::util::context::{Context, ContextStatics};
    use crate::asc::util::namespaces::Namespaces;

    fn run(v: i32) -> String {
        TypeValue::init();
        ObjectValue::init();
        let statics = Rc::new(RefCell::new(ContextStatics::new()));
        {
            let mut s = statics.borrow_mut();
            s.use_static_semantics = true;
            s.dialect = DIALECT_AS3;
            s.set_abc_version(TARGET_AVM2);
        }
        let cx = Rc::new(RefCell::new(Context::new(statics)));
        let mut src = Source { debuggable: v != 2, ..Default::default() };
        let (debug, keep) = (v != 3, v == 1);
        if v == 1 {
            src.name = "C:\\src\\Main.as".into();
            src.root = true;
        } else {
            src.name = "C:\\src\\mx\\controls\\Button.as".into();
            src.source_path_owner = true;
            src.relative_path = "mx/controls".into();
        }
        let map = (v == 0 || v == 2).then(|| {
            Rc::new(LineNumberMap {
                new_name: src.name.clone(),
                old_name: "C:\\src\\mx\\controls\\Button.mxml".into(),
                lines: [(10, 3), (12, 5), (13, 7)].into_iter().collect(),
            })
        });
        let src = Rc::new(src);
        let e = Rc::new(RefCell::new(new_bytecode_emitter_full(&cx, Some(src.clone()), debug, false, keep, map)));
        cx.borrow().set_emitter(Some(e.clone() as EmitterRef));
        {
            let c = cx.borrow();
            let global = ObjectValue::with_builder(&c, new_global_builder(), None);
            c.push_scope(global.clone());
            let n = Namespaces::of(c.public_namespace());
            let b = global.builder().unwrap();
            let ve = global.get_slot(&c, b.explicit_var_nss(&c, &global, "ve", &n, Some(&c.no_type()), -1, -1, -1)).unwrap();
            let m1 = MetaData::new();
            m1.set_id(Some("Embed".into()));
            m1.set_values(Some(vec![Value::KeyValuePair(Rc::new(KeyValuePair::new("source", "x.png")))]));
            let m2 = MetaData::new();
            m2.set_id(Some("Foo".into()));
            ve.add_metadata(Some(m1.clone()));
            ve.add_metadata(Some(m2));
            let vk = global.get_slot(&c, b.explicit_var_nss(&c, &global, "vk", &n, Some(&c.no_type()), -1, -1, -1)).unwrap();
            vk.add_metadata(Some(m1));
        }
        let mut e = e.borrow_mut();
        e.start_program("t");
        e.set_origin(&src.name);
        e.start_method("$init", 0, 0, 0, false, 0);
        e.set_position(10, 1, 100);
        e.get_global_scope();
        e.push_scope();
        e.set_position(11, 1, 110);
        e.push_null();
        e.pop();
        e.set_position(12, 1, 120);
        e.define_slot_variable(&cx, "v", "", 0, None, 1);
        e.push_null();
        e.pop();
        e.set_position(10, 1, 130);
        e.push_null();
        e.pop();
        e.set_origin("C:\\src\\other.as");
        e.set_position(13, 1, 140);
        e.push_null();
        e.pop();
        e.set_origin(&src.name);
        e.set_position(14, 1, 150);
        e.push_null();
        e.pop();
        e.return_(TYPE_void);
        let init = e.finish_method(&cx, "$init", None, None, None, 0, 1, "", false, false, None);
        e.finish_program(&cx, "", init);
        let mut bytes = Vec::new();
        e.emit(&mut bytes);
        let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        format!("{}\nlines={}\n", hex, src.line_count.get())
    }

    #[test]
    fn embed_filtering_and_debug_line_mapping_match_java() {
        if MAIN_SEPARATOR != '\\' {
            return;
        }
        let expected = [
            "10002e00000000081e433a5c7372633b6d785c636f6e74726f6c733b427574746f6e2e6d786d6c01760f433a5c7372635c6f746865722e61730002766503466f6f02766b02080400030701050701070100000400010600000100020140000000010002400000000001000101010220f101f00364302029ef01020105f0052029f0032029f103f00d2029f1012029470000\nlines=5\n",
            "10002e000000000b0f433a5c7372633b3b4d61696e2e6173017610433a5c7372633b3b6f746865722e61730002766505456d62656406736f7572636505782e706e6703466f6f02766b020804000307010507010a0100000400020601070809000001000201400000000200010240000000010001000101010224f101f00a6430f00b2029ef0102010cf00c2029f00a2029f103f00d2029f101f00e2029470000\nlines=5\n",
            "10002e00000000050002766503466f6f02766b0208010003070102070104010000010001030000010002014000000001000240000000000100010101020d643020292029202920292029470000\nlines=0\n",
            "10002e00000000050002766503466f6f02766b0208010003070102070104010000010001030000010002014000000001000240000000000100010101020d643020292029202920292029470000\nlines=0\n",
        ];
        for (v, exp) in expected.iter().enumerate() {
            assert_eq!(run(v as i32), *exp, "variant {}", v);
        }
    }
}
