//! Instruction translation: FFDec's `AVM2Code.toSourceOutput` and the per-instruction `translate`.

use std::collections::HashMap;

use super::code::Code;
use super::ctx::Ctx;
use super::graph::St;
use super::item::*;
use super::ld::{CodeStats, Ld};
use super::names::Dotted;
use crate::abc::MethodBody;

/// Per-method translation context (the immutable parts of `AVM2GraphSource`/`AVM2LocalData`).
pub struct Tr<'c, 'a> {
    pub ctx: &'c Ctx<'a>,
    pub body: &'a MethodBody,
    pub code: Code,
    pub method: usize,
    pub is_static: bool,
    pub script_index: i32,
    pub class_index: i32,
    pub local_reg_names: HashMap<i32, String>,
    pub fqn: Vec<Dotted>,
    pub swf_version: u8,
    pub call_stack: Vec<usize>,
    pub path: String,
}

impl<'c, 'a> Tr<'c, 'a> {
    /// `code.translatePart(output, ..., start, end)`.
    pub fn translate_range(&self, output: &List, ld: &mut Ld, stack: &St, start: usize, end: usize) {
        let _ = (output, ld, stack, start, end);
    }

    /// `AVM2Code.getStats(abc, body, initScope, false)`.
    pub fn stats(&self) -> CodeStats {
        CodeStats { ins: vec![Default::default(); self.code.len()], ..Default::default() }
    }
}
