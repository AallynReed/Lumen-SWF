//! AS3 decompiler: a port of FFDec's AVM2 graph decompiler.

pub mod avm2graph;
pub mod avm2post;
pub mod code;
pub mod ctx;
pub mod eq;
pub mod graph;
pub mod index;
pub mod ld;
pub mod item;
pub mod loops;
pub mod method;
pub mod names;
pub mod parts;
pub mod post;
pub mod print;
pub mod props;
pub mod script;
pub mod stack;
pub mod structure;
pub mod translate;
pub mod values;
pub mod writer;

use std::sync::Arc;

use crate::abc::scripts::Entry;
use crate::abc::Abc;

/// Type information for a SWF's ABCs; build once and reuse for every script.
pub fn index_for(abcs: &[&Abc]) -> Arc<index::AbcIndex> {
    index::AbcIndex::for_abcs(abcs)
}

/// Decompiled ActionScript of one script entry.
pub fn entry_source(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf_version: u8) -> String {
    let mut ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
    ctx.swf_version = swf_version;
    let pack = script::pack_of(&ctx, e.script, e.trait_index);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| script::pack_source(&ctx, &pack)));
    r.unwrap_or_else(|_| "/*\r\n * Decompilation error\r\n */\r\n".into())
}
