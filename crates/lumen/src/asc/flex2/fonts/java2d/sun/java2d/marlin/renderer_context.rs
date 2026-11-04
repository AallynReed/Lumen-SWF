//! The `sun.java2d.marlin.RendererContext` state the stroking pipeline shares, and `RendererContext.PathConsumer2DAdapter`.

use std::cell::Cell;
use std::rc::Rc;

use super::{DPathConsumer2D, PathConsumer2D};

/// `RendererContext` (only `doClip`, `clipRect`, `clipInvScale` and `closedPath`).
pub struct RendererContext {
    pub do_clip: bool,
    pub clip_rect: [f64; 4],
    pub clip_inv_scale: f64,
    pub closed_path: Rc<Cell<bool>>,
}

impl RendererContext {
    pub fn new() -> RendererContext {
        RendererContext { do_clip: false, clip_rect: [0.0; 4], clip_inv_scale: 0.0, closed_path: Rc::new(Cell::new(false)) }
    }
}

impl Default for RendererContext {
    fn default() -> Self {
        RendererContext::new()
    }
}

/// The fields of the thread's reused `RendererContext` that a stroking call can read before writing them: Java keeps
/// one context per thread (`USE_THREAD_LOCAL`), so these carry over from the previous call on the same thread.
#[derive(Clone, Copy, Default, Debug)]
pub struct CarriedState {
    /// `RendererContext.stroker.smx`, `smy`.
    pub stroker_smx: f64,
    pub stroker_smy: f64,
    /// `RendererContext.nPCPathIterator` `curx_adjust`, `cury_adjust`, `movx_adjust`, `movy_adjust`.
    pub npc_adjust: [f64; 4],
    /// `RendererContext.nPQPathIterator` `curx_adjust`, `cury_adjust`, `movx_adjust`, `movy_adjust`.
    pub npq_adjust: [f64; 4],
}

thread_local! {
    static CARRIED: Cell<CarriedState> = Cell::new(CarriedState::default());
}

/// The calling thread's `CarriedState`.
pub fn carried_state() -> CarriedState {
    CARRIED.with(|c| c.get())
}

/// Updates the calling thread's `CarriedState`.
pub fn update_carried_state(f: impl FnOnce(&mut CarriedState)) {
    CARRIED.with(|c| {
        let mut s = c.get();
        f(&mut s);
        c.set(s);
    });
}

/// `RendererContext.PathConsumer2DAdapter`.
pub struct PathConsumer2DAdapter<'a> {
    out: &'a mut dyn PathConsumer2D,
}

impl<'a> PathConsumer2DAdapter<'a> {
    pub fn new(out: &'a mut dyn PathConsumer2D) -> PathConsumer2DAdapter<'a> {
        PathConsumer2DAdapter { out }
    }
}

impl DPathConsumer2D for PathConsumer2DAdapter<'_> {
    fn move_to(&mut self, x0: f64, y0: f64) {
        self.out.move_to(x0 as f32, y0 as f32);
    }
    fn line_to(&mut self, x1: f64, y1: f64) {
        self.out.line_to(x1 as f32, y1 as f32);
    }
    fn close_path(&mut self) {
        self.out.close_path();
    }
    fn path_done(&mut self) {
        self.out.path_done();
    }
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        self.out.curve_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32, x3 as f32, y3 as f32);
    }
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.out.quad_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32);
    }
}
