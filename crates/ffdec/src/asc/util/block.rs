//! Port of `macromedia/asc/util/Block.java`.

use crate::asc::util::bit_set::BitSet;
use crate::asc::util::int_list::IntList;

#[derive(Clone, Default, Debug)]
pub struct Block {
    pub def_bits: Option<BitSet>,
    pub gen_bits: Option<BitSet>,
    pub kill_bits: Option<BitSet>,
    pub in_bits: Option<BitSet>,
    pub out_bits: Option<BitSet>,
    pub stmts: Option<String>,
    /// This block can not have a successor; it is the block jumped to by `return`.
    pub is_terminal: bool,
    pub preds: IntList,
    pub succs: IntList,
}

impl Block {
    pub fn new() -> Block {
        Block { preds: IntList::with_capacity(1), succs: IntList::with_capacity(1), ..Default::default() }
    }

    /// `block.succs.size()`.
    pub fn succs_size(&self) -> usize {
        self.succs.len()
    }
}
