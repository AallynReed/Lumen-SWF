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

/// What the decompiler needs to know about the SWF around an ABC.
#[derive(Debug, Clone, Default)]
pub struct SwfContext {
    pub version: u8,
    pub doc: Option<ctx::DocClass>,
}

impl SwfContext {
    pub fn of(swf: &crate::Swf) -> SwfContext {
        let names = crate::names::Names::new(swf);
        let doc = names.classes.get(&0).and_then(|v| v.first()).map(|name| {
            let r = &swf.frame_rect;
            let bg = swf.timeline.tags.iter().find(|t| t.code == crate::tags::code::SET_BACKGROUND_COLOR).and_then(|t| crate::types::Rgba::read_rgb(&mut crate::io::Reader::new(&t.body)).ok()).map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
            ctx::DocClass {
                name: name.clone(),
                width: ((r.x_max - r.x_min) as f64 / 20.0).round() as i64,
                height: ((r.y_max - r.y_min) as f64 / 20.0).round() as i64,
                background: bg,
                frame_rate: swf.frame_rate,
            }
        });
        SwfContext { version: swf.header.version, doc }
    }
}

/// Decompiled ActionScript of one script entry.
pub fn entry_source(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext) -> String {
    entry_source_with(index, abc, e, swf, ctx::Config::default())
}

/// `entry_source` with explicit decompiler settings.
pub fn entry_source_with(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext, cfg: ctx::Config) -> String {
    let r = with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, cfg).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let pack = script::pack_of(&ctx, e.script, e.trait_index);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| script::pack_source(&ctx, &pack))).ok()
    });
    r.unwrap_or_else(|| "/*\r\n * Decompilation error\r\n */\r\n".into())
}

/// The names an entry's decompiled text uses for identifiers that are not valid ActionScript,
/// as (printed name, name in the ABC).
pub fn entry_renames(index: &Arc<index::AbcIndex>, abc: &Abc, e: &Entry, swf: &SwfContext) -> Vec<(String, String)> {
    with_big_stack(|| {
        let mut ctx = ctx::Ctx::new(abc, ctx::Config::default()).with_index(index.clone());
        ctx.swf_version = swf.version;
        ctx.doc = swf.doc.clone();
        let pack = script::pack_of(&ctx, e.script, e.trait_index);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| script::pack_source(&ctx, &pack)));
        ctx.ids.renamed().into_iter().map(|(raw, printed)| (printed, raw)).collect()
    })
}

/// Runs `f` on a thread with a large stack: the structuring code recurses once per nested block.
pub fn with_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|s| std::thread::Builder::new().stack_size(1 << 30).spawn_scoped(s, f).expect("decompiler thread").join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
}
