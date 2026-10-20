//! Method body decompilation entry points.

use super::ctx::LocalData;
use super::item::It;
use super::writer::Writer;

pub fn print_new_function(_it: &It, w: &mut Writer, _ld: &LocalData) {
    w.append("function(){}");
}

pub fn print_trait_slot_const(_it: &It, _w: &mut Writer, _ld: &LocalData) {}
