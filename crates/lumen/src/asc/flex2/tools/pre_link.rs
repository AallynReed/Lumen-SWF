//! The part of `flex2.tools.PreLink` a pure-ActionScript application reaches: `processMainUnit` (main definition, root
//! class and `[SWF]` metadata). The Flex init, loader, style and resource-bundle code generation is not ported.

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};

/// `Integer.decode`.
fn integer_decode(s: &str) -> Option<i32> {
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = if let Some(r) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = rest.strip_prefix('#') {
        (16, r)
    } else if rest.starts_with('0') && rest.len() > 1 {
        (8, &rest[1..])
    } else {
        (10, rest)
    };
    if digits.is_empty() || digits.starts_with('-') || digits.starts_with('+') {
        return None;
    }
    let v = i64::from_str_radix(digits, radix).ok()?;
    let v = if neg { -v } else { v };
    i32::try_from(v).ok()
}

/// `Integer.parseInt`.
fn parse_int(s: &str) -> Option<i32> {
    s.parse::<i32>().ok()
}

fn could_not_parse(num: &str, attribute: &str) {
    toolkit::log(CompilerMessage::error(format!("Could not parse '{num}' as a number for {attribute}.")), None, -1, -1, None);
}

/// `swfmetadata(u, cfg)`.
fn swfmetadata(u: &UnitRef, cfg: &mut Configuration) {
    let Some(md) = u.borrow().swf_meta_data.clone() else { return };
    let v = |k: &str| md.get_value_key(k);
    if let Some(w) = v("width") {
        cfg.width = Some(w);
    }
    if let Some(h) = v("height") {
        cfg.height = Some(h);
    }
    if let Some(w) = v("widthPercent") {
        cfg.width_percent = Some(w);
    }
    if let Some(h) = v("heightPercent") {
        cfg.height_percent = Some(h);
    }
    if let Some(s) = v("scriptRecursionLimit") {
        match parse_int(&s) {
            Some(n) => cfg.script_recursion_limit = n,
            None => could_not_parse(&s, "scriptRecursionLimit"),
        }
    }
    if let Some(s) = v("scriptTimeLimit") {
        match parse_int(&s) {
            Some(n) => cfg.script_time_limit = n,
            None => could_not_parse(&s, "scriptTimeLimit"),
        }
    }
    if let Some(s) = v("frameRate") {
        match parse_int(&s) {
            Some(n) => cfg.frame_rate = n,
            None => could_not_parse(&s, "frameRate"),
        }
    }
    if let Some(s) = v("backgroundColor") {
        match integer_decode(&s) {
            Some(n) => cfg.background_color = n,
            None => {
                let name = u.borrow().get_source().borrow().get_name_for_reporting().to_string();
                toolkit::log_for(CompilerMessage::error(format!("Invalid background color: {s}")), &name);
            }
        }
    }
    if let Some(s) = v("pageTitle") {
        cfg.page_title = Some(s);
    }
    if let Some(s) = v("useDirectBlit") {
        cfg.use_direct_blit = s.eq_ignore_ascii_case("true");
    }
    if let Some(s) = v("useGPU") {
        cfg.use_gpu = s.eq_ignore_ascii_case("true");
    }
}

/// `PreLink.run`: false, as no sources are generated for a pure-ActionScript root.
pub fn run(units: &[UnitRef], configuration: &mut Configuration) -> bool {
    for u in units {
        if !u.borrow().is_root() {
            continue;
        }
        swfmetadata(u, configuration);
        let loader = u.borrow().loader_class.clone();
        if let Some(loader) = loader {
            configuration.root_class_name = Some(loader);
        }
        let last = u.borrow().top_level_definitions.last().map(|q| q.to_string());
        if let Some(def) = last {
            configuration.main_definition = Some(def.clone());
            u.borrow_mut().context.main_definition = Some(def);
        }
    }
    false
}
