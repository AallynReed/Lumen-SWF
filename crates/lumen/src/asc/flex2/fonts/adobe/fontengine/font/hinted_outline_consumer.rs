//! Port of afe.jar `com.adobe.fontengine.font.HintedOutlineConsumer`.

use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;

pub trait HintedOutlineConsumer: OutlineConsumer {
    fn stem(&mut self, edge1: f64, edge2: f64, doing_hints: bool, vertical: bool, is_counter: bool);
    #[allow(clippy::too_many_arguments)]
    fn stem3(&mut self, e1: f64, e2: f64, e3: f64, e4: f64, e5: f64, e6: f64, doing_hints: bool, vertical: bool);
    fn global_color_on(&mut self);
    fn no_counters(&mut self);
    fn no_hints(&mut self);
    fn width(&mut self, w: f64) -> bool;
    fn closepath(&mut self);
    #[allow(clippy::too_many_arguments)]
    fn flex(&mut self, depth: f64, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64, x5: f64, y5: f64, x6: f64, y6: f64);
}
