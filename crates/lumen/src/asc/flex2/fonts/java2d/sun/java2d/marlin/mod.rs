//! Port of the JDK 17 Marlin stroking pipeline (`sun.java2d.marlin`) as `LoopPipe.getStrokeSpans` reaches it through
//! `DMarlinRenderingEngine.strokeTo`.

pub mod curve;
pub mod d_marlin_rendering_engine;
pub mod dasher;
pub mod float_math;
pub mod helpers;
pub mod marlin_const;
pub mod math;
pub mod renderer_context;
pub mod stroker;
pub mod transforming_path_consumer2d;

pub use d_marlin_rendering_engine::stroke_to;

/// `sun.awt.geom.PathConsumer2D` (float coordinates).
pub trait PathConsumer2D {
    fn move_to(&mut self, x: f32, y: f32);
    fn line_to(&mut self, x: f32, y: f32);
    fn quad_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32);
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32);
    fn close_path(&mut self);
    fn path_done(&mut self);
}

/// `sun.java2d.marlin.DPathConsumer2D`.
pub trait DPathConsumer2D {
    fn move_to(&mut self, x0: f64, y0: f64);
    fn line_to(&mut self, x1: f64, y1: f64);
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64);
    fn quad_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64);
    fn close_path(&mut self);
    fn path_done(&mut self);
}
