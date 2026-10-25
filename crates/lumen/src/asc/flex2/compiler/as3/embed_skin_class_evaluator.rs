//! Port of `flex2.compiler.as3.EmbedSkinClassEvaluator`: in parse1, makes each `[Embed(skinClass=...)]` class an
//! inheritance dependency, so it is parsed before `EmbedEvaluator` looks it up.

use super::evaluator_adapter::Adapter;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::transcoder::SKINCLASS;
use crate::asc::flex2::compiler::util::{name_formatter, MultiName, Name};
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::util::context::ContextRef;

pub struct EmbedSkinClassEvaluator {
    unit: UnitRef,
}

impl EmbedSkinClassEvaluator {
    pub fn new(unit: UnitRef) -> EmbedSkinClassEvaluator {
        EmbedSkinClassEvaluator { unit }
    }
}

impl Adapter for EmbedSkinClassEvaluator {
    fn visit(&mut self, _cx: &ContextRef, node: &NodeRef) -> bool {
        if !matches!(node.borrow().kind, Kind::MetaData(_)) {
            return false;
        }
        let b = node.borrow();
        let m = b.as_meta_data().unwrap();
        if m.get_id().as_deref() == Some("Embed") {
            let values = m.get_metadata().and_then(|md| md.values()).unwrap_or_default();
            for v in values {
                if let crate::asc::semantics::value::Value::KeyValuePair(kv) = v {
                    if kv.key == SKINCLASS {
                        self.unit.borrow_mut().inheritance.insert(Name::M(MultiName::parse(&name_formatter::to_colon(&kv.obj))));
                    }
                }
            }
        }
        true
    }
}
