//! The `TypeTable` dump `scratchpad\mxml\reflect\ReflectDump.java` makes, from the Rust port. Runs when
//! `REFLECT_DUMP=<out>` and `REFLECT_DIR=<dir with src/ and classes.txt>` are set.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::rc::Rc;

use super::type_table::{Type, TypeRef, TypeTable};
use crate::asc::flex2::compiler::as3::reflect::meta_data::MetaData;
use crate::asc::flex2::compiler::compiler_api::{CompilerApi, DependencyGraph};
use crate::asc::flex2::compiler::compiler_swc_context::CompilerSwcContext;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs;
use crate::asc::flex2::compiler::source::{canonical_path, VirtualFile};
use crate::asc::flex2::compiler::source_path::{SourceList, SourcePath};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::tools::mxmlc;
use crate::asc::util::jhash::java_compare;

const SDK: &str = r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk\frameworks\libs";

fn tn(t: Option<TypeRef>) -> String {
    t.map_or("null".to_string(), |t| t.get_name())
}

fn s<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or("null".to_string(), |v| v.to_string())
}

fn arr<T: AsRef<str>>(a: Option<&[T]>) -> String {
    match a {
        None => "null".to_string(),
        Some(a) => format!("[{}]", a.iter().map(|x| x.as_ref().to_string()).collect::<Vec<_>>().join(", ")),
    }
}

fn arr_opt(a: Option<Vec<Option<String>>>) -> String {
    match a {
        None => "null".to_string(),
        Some(a) => format!("[{}]", a.iter().map(|x| s(x.as_deref())).collect::<Vec<_>>().join(", ")),
    }
}

fn mdn(l: Option<Vec<MetaData>>) -> String {
    let Some(l) = l else { return "null".to_string() };
    let mut b = String::new();
    for m in l {
        b.push('[');
        b.push_str(&s(m.get_id()));
        for i in 0..m.count() {
            let _ = write!(b, " {}={}", s(m.get_key(i)), s(m.get_value_index(i)));
        }
        b.push(']');
    }
    b
}

const BASES: &[&str] = &[
    "Object", "*", "flash.events:IEventDispatcher", "flash.events:EventDispatcher", "flash.display:DisplayObject", "mx.core:UIComponent", "mx.core:IUIComponent",
    "mx.core:IVisualElement", "mx.core:Repeater", "mx.core:IDeferredInstance", "spark.components:Button", "spark.components.supportClasses:SkinnableComponent",
    "mx.core:IFactory", "Array", "test:IThing", "test:Foo", "no:Such",
];

fn b01(v: bool) -> char {
    if v {
        '1'
    } else {
        '0'
    }
}

fn try_s(o: &mut String, f: impl FnOnce() -> String) -> bool {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => {
            o.push_str(&v);
            true
        }
        Err(_) => false,
    }
}

fn dump_class(o: &mut String, st: &SymbolTable, tt: &TypeTable, n: &str) {
    let _ = writeln!(o, "C {n}");
    let Some(t) = tt.get_type(n) else {
        o.push_str("  nulltype\n");
        return;
    };
    let c = st.get_class(Some(n)).unwrap();
    let _ = writeln!(o, "  name={} super={} elem={}", t.get_name(), tn(t.get_super_type()), tn(t.get_element_type()));
    let mut b = String::from("  ifaces=");
    match t.get_interfaces() {
        None => b.push_str("null"),
        Some(ifs) => {
            for i in ifs {
                b.push_str(&tn(i));
                b.push(',');
            }
        }
    }
    let _ = writeln!(o, "{b}");
    let max = catch_unwind(AssertUnwindSafe(|| t.get_max_children().to_string())).unwrap_or_else(|_| "EXC".to_string());
    let line = format!(
        "  dyn={} max={} loader={} default={} obs={} md={}/{} iface={} pub={}",
        t.has_dynamic(),
        max,
        s(t.get_loader_class()),
        t.get_default_property().map_or("null".to_string(), |p| s(p.get_name())),
        t.has_obsolete("ancient"),
        t.has_metadata("Event", true),
        t.has_metadata("Event", false),
        c.is_interface(),
        c.is_public()
    );
    let _ = writeln!(o, "{line}");
    let mut b = String::from("  assign=");
    for base in BASES {
        let bt = tt.get_type(base);
        b.push(b01(t.is_assignable_to_name(Some(base))));
        b.push(b01(t.is_subclass_of_name(Some(base))));
        b.push(b01(t.is_assignable_to(bt.as_deref())));
        b.push(b01(t.is_subclass_of(bt.as_deref())));
        b.push(b01(c.implements_interface(base)));
        b.push(' ');
    }
    let _ = writeln!(o, "{b}");
    let mut props: Vec<String> = Vec::new();
    let mut add = |props: &mut Vec<String>, n: String| {
        if !props.contains(&n) {
            props.push(n);
        }
    };
    let mut k = Some(c.clone());
    while let Some(kc) = k {
        let top = Rc::ptr_eq(&kc, &c);
        for v in kc.get_var_iterator() {
            if top {
                let _ = writeln!(
                    o,
                    "  V {} {} {} {}{}{} {} {}",
                    v.get_qname(),
                    s(v.get_type_name()),
                    s(v.get_element_type_name()),
                    v.is_const(),
                    v.is_static(),
                    v.is_public(),
                    v.get_declaring_class_name(),
                    mdn(v.get_meta_data("Bindable"))
                );
            }
            add(&mut props, v.get_qname().local_part.clone());
        }
        for m in kc.get_getter_iterator() {
            if top {
                let _ = writeln!(o, "  G {} {} {} {}{} {}", m.get_qname(), s(m.get_return_type_name()), s(m.get_return_element_type_name()), m.is_static(), m.is_public(), mdn(m.get_meta_data("Bindable")));
            }
            add(&mut props, m.get_qname().local_part.clone());
        }
        for m in kc.get_setter_iterator() {
            if top {
                let _ = writeln!(o, "  S {} {} {} {}{}", m.get_qname(), arr_opt(m.get_parameter_type_names()), arr_opt(m.get_parameter_element_type_names()), m.is_static(), m.is_public());
            }
            add(&mut props, m.get_qname().local_part.clone());
        }
        for m in kc.get_method_iterator() {
            if top {
                let _ = writeln!(o, "  M {} {} {} {}{}", m.get_qname(), s(m.get_return_type_name()), arr_opt(m.get_parameter_type_names()), m.is_static(), m.is_public());
            }
            add(&mut props, m.get_qname().local_part.clone());
        }
        k = st.get_class(kc.get_super_type_name());
    }
    add(&mut props, "nonexistent".to_string());
    for pn in &props {
        let mut l = format!("  P {pn} static={}", t.has_static_member(pn));
        let ok = {
            let l = &mut l;
            catch_unwind(AssertUnwindSafe(|| {
                let pr = t.get_property(pn);
                let pubp = t.get_property_ns("", pn);
                let _ = write!(l, " pub={}", pubp.map_or("null".to_string(), |p| s(p.get_name())));
                match pr {
                    None => l.push_str(" null"),
                    Some(pr) => {
                        l.push_str(" name=");
                        l.push_str(&s(pr.get_name()));
                        l.push_str(" type=");
                        l.push_str(&tn(pr.get_type()));
                        let _ = write!(
                            l,
                            " ro={} hp={} pp={} rich={} col={} ce={}{}",
                            pr.read_only(),
                            pr.has_public(),
                            s(pr.get_percent_proxy()),
                            pr.rich_text_content(),
                            pr.collapse_white_space(),
                            pr.has_change_event("fooChange"),
                            pr.has_change_event("other")
                        );
                        if let Some(ins) = pr.get_inspectable() {
                            let _ = write!(
                                l,
                                " insp={}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                                arr(ins.get_enumeration()),
                                s(ins.get_default_value()),
                                ins.is_default(),
                                s(ins.get_category()),
                                ins.is_verbose(),
                                s(ins.get_type()),
                                s(ins.get_object_type()),
                                s(ins.get_array_type()),
                                s(ins.get_environment()),
                                s(ins.get_format())
                            );
                        }
                        if let Some(d) = pr.get_deprecated() {
                            let _ = write!(l, " dep={}|{}|{}", s(d.get_replacement()), s(d.get_message()), s(d.get_since()));
                        }
                        l.push_str(" inst=");
                        if !try_s(l, || tn(pr.get_instance_type())) {
                            l.push_str(" inst=EXC");
                        }
                        l.push_str(" elem=");
                        if !try_s(l, || tn(pr.get_element_type())) {
                            l.push_str(" elem=EXC");
                        }
                        l.push_str(" lv=");
                        if !try_s(l, || tn(pr.get_lvalue_type())) {
                            l.push_str(" lv=EXC");
                        }
                    }
                }
            }))
            .is_ok()
        };
        if !ok {
            l.push_str(" EXC");
        }
        let _ = writeln!(o, "{l}");
    }
    let mut events: Vec<Option<String>> = Vec::new();
    for md in c.get_meta_data("Event", true) {
        let e = md.get_value_key("name").or_else(|| md.get_value_index(0));
        if !events.contains(&e) {
            events.push(e);
        }
    }
    if !events.contains(&Some("nonexistent".to_string())) {
        events.push(Some("nonexistent".to_string()));
    }
    for en in &events {
        let en = en.as_deref().unwrap_or("null");
        let line = match t.get_event(en) {
            None => " null".to_string(),
            Some(e) => format!(
                " {} {} {} {}|{}|{}",
                s(e.get_name()),
                e.get_type_name(),
                tn(e.get_type()),
                s(e.get_deprecated_message()),
                s(e.get_deprecated_replacement()),
                s(e.get_deprecated_since())
            ),
        };
        let _ = writeln!(o, "  E {en}{line}");
    }
    let mut styles: Vec<Option<String>> = Vec::new();
    for md in c.get_meta_data("Style", true) {
        let e = md.get_value_key("name");
        if !styles.contains(&e) {
            styles.push(e);
        }
    }
    if !styles.contains(&Some("nonexistent".to_string())) {
        styles.push(Some("nonexistent".to_string()));
    }
    for sn in &styles {
        let Some(sn) = sn else {
            o.push_str("  T null\n");
            continue;
        };
        let r = match t.get_style(sn) {
            None => " null".to_string(),
            Some(st) => format!(
                " {} {} {} {} {} {} {} {}|{}|{}",
                st.get_name(),
                tn(st.get_type()),
                tn(st.get_lvalue_type()),
                tn(st.get_element_type()),
                arr(st.get_enumeration()),
                s(st.get_format()),
                st.is_inherit(),
                s(st.get_deprecated_message()),
                s(st.get_deprecated_replacement()),
                s(st.get_deprecated_since())
            ),
        };
        let _ = writeln!(o, "  T {sn}{r} themes={} ex={}", s(t.get_style_themes(sn)), t.is_excluded_style(sn));
    }
    let mut effects: Vec<Option<String>> = Vec::new();
    for md in c.get_meta_data("Effect", true) {
        let e = if md.count() > 0 { md.get_value_index(0) } else { None };
        if !effects.contains(&e) {
            effects.push(e);
        }
    }
    if !effects.contains(&Some("nonexistent".to_string())) {
        effects.push(Some("nonexistent".to_string()));
    }
    for fname in &effects {
        let Some(fname) = fname else {
            o.push_str("  F null\n");
            continue;
        };
        let line = match t.get_effect(fname) {
            None => " null".to_string(),
            Some(f) => format!(
                " {} {} {} {} {}|{}|{}",
                f.get_name(),
                s(f.get_event()),
                tn(f.get_type()),
                tn(f.get_lvalue_type()),
                s(f.get_deprecated_message()),
                s(f.get_deprecated_replacement()),
                s(f.get_deprecated_since())
            ),
        };
        let _ = writeln!(o, "  F {fname}{line}");
    }
}

fn dump(st: &SymbolTable, tt: &TypeTable) -> String {
    let mut o = String::new();
    let mut names = st.get_class_names();
    names.sort_by(|a, b| java_compare(a, b));
    let _ = writeln!(o, "CLASSES {}", names.len());
    for n in &names {
        let mut part = String::new();
        let r = catch_unwind(AssertUnwindSafe(|| dump_class(&mut part, st, tt, n)));
        o.push_str(&part);
        if r.is_err() {
            o.push_str("  EXC java.lang.NullPointerException\n");
        }
    }
    let _ = writeln!(o, "VECTOR {} {} {}", tn(tt.get_type("__AS3__.vec:Vector.<String>")), tn(tt.get_type("__AS3__.vec:Vector.<spark.components:Button>")), tn(tt.get_type("__AS3__.vec:Vector.<no:Such>")));
    if let Some(vs) = tt.get_type("__AS3__.vec:Vector.<String>") {
        let _ = writeln!(o, "  elem {} super {} vt {}", tn(vs.get_element_type()), tn(vs.get_super_type()), tt.get_vector_type(&tt.number_type()).get_name());
    }
    let _ = writeln!(o, "GLOBALSTYLE {}", if tt.get_style("color").is_none() { "null" } else { "found" });
    let dp = tt.get_dynamic_property("dp", Some("s1"));
    use super::type_table::Stateful;
    let _ = writeln!(o, "DYN {} {}", tn(dp.get_type()), s(dp.get_state_name()));
    o
}

fn run(dir: PathBuf, out: PathBuf) {
    let src = dir.join("src");
    let classes = std::fs::read_to_string(dir.join("classes.txt")).unwrap();
    let mut configuration = Configuration::default();
    configuration.source_path = vec![src.clone()];
    configuration.library_path = ["framework.swc", "spark.swc", r"mx\mx.swc", "textLayout.swc", "rpc.swc"].iter().map(|l| PathBuf::from(SDK).join(l)).collect();
    configuration.external_library_path = vec![PathBuf::from(SDK).join(r"player\11.1\playerglobal.swc")];
    configuration.includes = classes.lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    toolkit::reset(true);
    let main = VirtualFile::local(&src.join("Main.as"));
    let class_path: Vec<String> = configuration.source_path.iter().map(|p| canonical_path(p)).collect();
    let source_list = SourceList::new(std::slice::from_ref(&main), &class_path, Some(&main)).ok().unwrap();
    let source_path = SourcePath::new(&class_path, Some(&main), &mxmlc::source_path_mime_types(), false);
    let mut swc_context = CompilerSwcContext::new();
    swc_context.load_with(&configuration.library_path, &configuration.external_library_path, Vec::new());
    configuration.add_externs(swc_context.get_externs());
    let config_rc = Rc::new(configuration.clone());
    let mut api = CompilerApi {
        sources: Vec::new(),
        units: Vec::new(),
        igraph: DependencyGraph::default(),
        symbol_table: SymbolTable::new(&configuration),
        compilers: mxmlc::compilers(&config_rc),
        source_list: Some(source_list),
        source_path: Some(source_path),
        swc_context,
        configuration,
    };
    let _ = api.compile(&mut |_| false);
    for d in toolkit::take_diagnostics() {
        eprintln!("{}", d.text);
    }
    let mut themes = HashSet::new();
    themes.insert("spark".to_string());
    let tt = TypeTable::new(&api.symbol_table, Rc::new(NameMappings::new()), StandardDefs::get_standard_defs("halo"), Some(themes));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let text = dump(&api.symbol_table, &tt);
    let mut before = String::new();
    dump_class(&mut before, &api.symbol_table, &tt, "test:Bar");
    for n in ["test:Foo", "test:Bar"] {
        api.symbol_table.get_class(Some(n)).unwrap().freeze();
    }
    let mut after = String::new();
    dump_class(&mut after, &api.symbol_table, &tt, "test:Bar");
    std::panic::set_hook(prev);
    assert_eq!(before, after, "freeze changed the view");
    std::fs::write(out, text).unwrap();
    tt.clear();
}

#[test]
fn reflect_dump() {
    let (Ok(out), Ok(dir)) = (std::env::var("REFLECT_DUMP"), std::env::var("REFLECT_DIR")) else { return };
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(move || run(PathBuf::from(dir), PathBuf::from(out))).unwrap();
    h.join().unwrap();
}

#[allow(dead_code)]
fn _type_is_css(t: &Type) -> &dyn crate::asc::flex2::compiler::css::document::CssType {
    t
}
