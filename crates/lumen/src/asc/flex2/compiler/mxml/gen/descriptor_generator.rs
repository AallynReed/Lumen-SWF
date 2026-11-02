//! Port of `flex2.compiler.mxml.gen.DescriptorGenerator`: `UIComponentDescriptor` trees for MX containers.

use std::collections::HashSet;

use super::code_fragment_list::CodeFragmentList;
use super::text_gen;
use crate::asc::flex2::compiler::mxml::rep::model::ModelRef;
use crate::asc::flex2::compiler::util::name_formatter;

const INDENT: &str = "  ";

/// `addDescriptorInitializerFragments(list, model, includePropNames, includeDesignLayer, indent)`: properties only.
pub fn add_descriptor_initializer_fragments_props(list: &mut CodeFragmentList, model: &ModelRef, include_prop_names: &HashSet<String>, include_design_layer: bool, indent: &str) {
    add_fragments(list, model, indent, Some(include_prop_names), include_design_layer, true);
}

/// `addDescriptorInitializerFragments(list, model, indent)`.
pub fn add_descriptor_initializer_fragments(list: &mut CodeFragmentList, model: &ModelRef, indent: &str) {
    add_fragments(list, model, indent, None, true, false);
}

fn add_fragments(list: &mut CodeFragmentList, model: &ModelRef, indent: &str, include_prop_names: Option<&HashSet<String>>, include_design_layer: bool, props_only: bool) {
    model.set_described(true);
    let mut indent = indent.to_string();
    list.add4(&indent, "new ", &name_formatter::to_dot(&model.get_standard_defs().class_uicomponentdescriptor), "({", 0);
    indent.push_str(INDENT);
    list.add3(&indent, "type: ", &name_formatter::to_dot(&model.get_type().get_name().to_string()), model.get_xml_line_number());
    if model.is_declared() {
        list.add2(&indent, ",", 0);
        list.add3(&indent, "id: ", &text_gen::quote_word(&model.get_id().unwrap_or_default()), model.get_xml_line_number());
    }
    if !props_only {
        add_descriptor_events(list, model, &indent);
        add_descriptor_effect_names(list, model, &indent);
        add_descriptor_styles_and_effects(list, model, &indent);
    }
    add_descriptor_properties(list, model, include_prop_names, include_design_layer, &indent);
    indent.truncate(indent.len() - INDENT.len());
    list.add2(&indent, "})", 0);
}

fn add_descriptor_properties(list: &mut CodeFragmentList, model: &ModelRef, include_prop_names: Option<&HashSet<String>>, include_design_layer: bool, indent: &str) {
    let props: Vec<_> = model.get_property_initializer_iterator_bindings(false).into_iter().filter(|i| include_prop_names.is_none_or(|n| n.contains(&i.get_name()))).collect();
    let children = if model.is_movie_clip() && model.has_children() { model.children() } else { Vec::new() };
    let layer_parent = model.layer_parent.borrow().clone();
    let has_design_layer = include_design_layer && layer_parent.is_some() && model.get_type().is_assignable_to_name(Some(&model.get_standard_defs().interface_ivisualelement));

    if !props.is_empty() || !children.is_empty() || has_design_layer {
        let mut indent = indent.to_string();
        if !list.is_empty() {
            list.add2(&indent, ",", 0);
        }
        list.add2(&indent, "propertiesFactory: function():Object { return {", 0);
        indent.push_str(INDENT);
        for (i, init) in props.iter().enumerate() {
            if !init.is_state_specific() {
                let more = i + 1 < props.len() || !children.is_empty() || has_design_layer;
                list.add5(&indent, &init.get_name(), ": ", &init.get_value_expr(), if more { "," } else { "" }, init.get_line_ref());
            }
        }
        if has_design_layer {
            let id = layer_parent.as_ref().and_then(|l| l.get_id()).unwrap_or_default();
            list.add5(&indent, "designLayer", ": ", &id, if !children.is_empty() { "," } else { "" }, model.get_xml_line_number());
        }
        if !children.is_empty() {
            list.add2(&indent, "childDescriptors: [", 0);
            let mut is_first = true;
            for init in &children {
                let child = init.as_value().and_then(|v| v.get_model()).expect("ClassCastException");
                if child.is_descriptor_init() {
                    if !is_first {
                        list.add2(&indent, ",", 0);
                    }
                    add_descriptor_initializer_fragments(list, &child, &format!("{indent}{INDENT}"));
                    is_first = false;
                }
            }
            list.add2(&indent, "]", 0);
        }
        indent.truncate(indent.len() - INDENT.len());
        list.add2(&indent, "}}", 0);
    }
}

fn add_descriptor_styles_and_effects(list: &mut CodeFragmentList, model: &ModelRef, indent: &str) {
    let inits = model.get_style_and_effect_initializer_iterator();
    if !inits.is_empty() {
        let mut indent = indent.to_string();
        if !list.is_empty() {
            list.add2(&indent, ",", 0);
        }
        list.add2(&indent, "stylesFactory: function():void {", 0);
        indent.push_str(INDENT);
        for init in &inits {
            list.add5(&indent, "this.", &init.get_name(), " = ", &format!("{};", init.get_value_expr()), init.get_line_ref());
        }
        indent.truncate(indent.len() - INDENT.len());
        list.add2(&indent, "}", 0);
    }
}

fn add_descriptor_effect_names(list: &mut CodeFragmentList, model: &ModelRef, indent: &str) {
    let effect_event_names = model.get_effect_names();
    if !effect_event_names.is_empty() {
        if !list.is_empty() {
            list.add2(indent, ",", 0);
        }
        list.add4(indent, "effects: [ ", &effect_event_names, " ]", model.get_xml_line_number());
    }
}

fn add_descriptor_events(list: &mut CodeFragmentList, model: &ModelRef, indent: &str) {
    let events = model.get_event_initializer_iterator();
    if !events.is_empty() {
        let mut indent = indent.to_string();
        if !list.is_empty() {
            list.add2(&indent, ",", 0);
        }
        list.add2(&indent, "events: {", 0);
        indent.push_str(INDENT);
        for (i, init) in events.iter().enumerate() {
            list.add5(&indent, &init.get_name(), ": ", &text_gen::quote_word(&init.get_value_expr()), if i + 1 < events.len() { "," } else { "" }, init.get_line_ref());
        }
        indent.truncate(indent.len() - INDENT.len());
        list.add2(&indent, "}", 0);
    }
}
