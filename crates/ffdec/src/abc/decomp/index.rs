//! Type information about classes and properties: FFDec's `AbcIndexing`.

use super::ctx::{Ctx, LocalData};
use super::item::*;
use crate::abc::Multiname;

/// `AbcIndexing.multinameToType`; `None` for cyclic type names.
pub fn multiname_to_type(ctx: &Ctx, mn: usize) -> Option<It> {
    multiname_to_type_depth(ctx, mn, 0)
}

fn multiname_to_type_depth(ctx: &Ctx, mn: usize, depth: u8) -> Option<It> {
    if mn == 0 {
        return Some(unbounded());
    }
    if depth > 16 {
        return None;
    }
    let names = ctx.names();
    match names.mn(mn)? {
        Multiname::TypeName { name, params } => {
            let obj = multiname_to_type_depth(ctx, *name, depth + 1)?;
            let mut ps = Vec::new();
            for &p in params {
                let r = multiname_to_type_depth(ctx, p, depth + 1)?;
                ps.push(if p == 0 { null(None, None) } else { r });
            }
            Some(bare(K::ApplyType { obj, params: new_list(ps) }, prec::PRIMARY))
        }
        Multiname::QName { ns, .. } if *ns != 0 && names.ns_kind(*ns) == crate::abc::ns_kind::PRIVATE => {
            let n = names.name(mn, &[], true, true);
            Some(bare(K::Type { full: super::names::Dotted::parse_with_suffix(&n), ns: Some(format!("ns:{ns}")) }, prec::NOPRECEDENCE))
        }
        _ => Some(ty_dotted(names.name_with_ns(mn, true))),
    }
}

/// `multinameToType` with Java's null mapped to `*`.
pub fn mn_type(ctx: &Ctx, mn: usize) -> It {
    multiname_to_type(ctx, mn).unwrap_or_else(unbounded)
}

/// Result of `findPropertyTypeOrCallType`.
pub struct PropType {
    pub ty: It,
    pub call_ty: It,
    pub is_static: bool,
}

/// `AbcIndexing.findPropertyTypeOrCallType`.
#[allow(clippy::too_many_arguments)]
pub fn find_property_type(_ctx: &Ctx, _owner: &It, _name: &str, _ns: usize, _find_static: bool, _find_instance: bool, _find_protected: bool) -> Option<PropType> {
    None
}

/// `AbcIndexing.findClass(...) != null`.
pub fn find_class(_ctx: &Ctx, _ty: &It, _script: i32) -> bool {
    false
}

/// `AbcIndexing.findScriptProperty`: (return type, call return type).
pub fn find_script_property(_ctx: &Ctx, _name: &str, _ns: &super::names::Dotted) -> Option<(It, It)> {
    None
}

/// `AbcIndexing.isPropertyAmbiguous` for a multiname property of `parent`.
pub fn is_property_ambiguous(_ld: &LocalData, _mn: usize, _parent: &It) -> Option<bool> {
    None
}

pub fn is_property_ambiguous_name(_ld: &LocalData, _name: &str, _parent: &It) -> Option<bool> {
    None
}
