//! `ClassDef.vm` merged over a rep tree built by hand the way the builders build it, compared with mxmlc's
//! `-generated.as`. Runs when `REP_MERGE_DIR` names a scratch dir holding `App-generated.as` from mxmlc.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use super::line_number_map::DualModeLineNumberMap;
use super::template_values;
use super::velocity_util::{self, VelocityUtil};
use crate::asc::flex2::compiler::compiler_api::{CompilerApi, DependencyGraph};
use crate::asc::flex2::compiler::compiler_swc_context::CompilerSwcContext;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::mxml::mxml_configuration::MxmlConfiguration;
use crate::asc::flex2::compiler::mxml::dom::node::Node;
use crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs as ReflectDefs;
use crate::asc::flex2::compiler::mxml::reflect::type_table::TypeTable;
use crate::asc::flex2::compiler::mxml::rep::deps::StandardDefs;
use crate::asc::flex2::compiler::mxml::rep::document_info::DocumentInfo;
use crate::asc::flex2::compiler::mxml::rep::model::{Model, RValue};
use crate::asc::flex2::compiler::mxml::rep::model::ModelRef;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::{MxmlDocument, MxmlDocumentRef};
use crate::asc::flex2::compiler::mxml::rep::script::Script;
use crate::asc::flex2::compiler::source::{canonical_path, Owner, Source, VirtualFile};
use crate::asc::flex2::compiler::source_path::{SourceList, SourcePath};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;
use crate::asc::flex2::tools::mxmlc;
use crate::asc::flex2::velocity;

const SDK: &str = r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk\frameworks\libs";

fn standard_defs() -> Rc<StandardDefs> {
    StandardDefs::get_standard_defs("halo")
}

fn type_table(dir: &PathBuf) -> (CompilerApi, Rc<TypeTable>) {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("Main.as"), "package { import flash.display.Sprite; public class Main extends Sprite {} }").unwrap();
    let mut configuration = Configuration::default();
    configuration.source_path = vec![src.clone()];
    configuration.library_path = ["framework.swc", "spark.swc", "textLayout.swc", r"mx\mx.swc"].iter().map(|l| PathBuf::from(SDK).join(l)).collect();
    configuration.external_library_path = vec![PathBuf::from(SDK).join(r"player\11.1\playerglobal.swc")];
    configuration.includes = ["spark.components:Application", "spark.components:Button", "mx.core:DeferredInstanceFromFunction", "mx.core:IFlexModule", "mx.core:IUIComponent", "mx.core:Application", "mx.containers:VBox", "mx.controls:Button", "spark.components:Label", "spark.components:Group", "spark.primitives:Rect", "mx.states:State", "mx.states:AddItems", "mx.states:SetProperty", "mx.states:SetStyle", "mx.states:SetEventHandler"].iter().map(|s| s.to_string()).collect();
    toolkit::reset(true);
    let main = VirtualFile::local(&src.join("Main.as"));
    let class_path: Vec<String> = vec![canonical_path(&src)];
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
    let mut themes = HashSet::new();
    themes.insert("spark".to_string());
    let tt = TypeTable::new(&api.symbol_table, Rc::new(NameMappings::new()), ReflectDefs::get_standard_defs("halo"), Some(themes));
    (api, tt)
}

fn mask_time(s: &str) -> String {
    s.lines().map(|l| if l.starts_with(" *  Time:") { " *  Time: -" } else { l }).collect::<Vec<_>>().join("\n")
}

/// Builds `<dir>/App.mxml`'s rep tree with `build` (given the document and type table, returning the root), merges
/// `ClassDef.vm` and compares it with `<dir>/App-generated.as`.
fn merge_and_compare(case: &str, super_class: &str, root_ns: &str, build: impl FnOnce(&MxmlDocumentRef, &TypeTable) -> ModelRef) {
    let Ok(base) = std::env::var("REP_MERGE_DIR") else { return };
    let dir = PathBuf::from(base).join(case);
    let expected = std::fs::read_to_string(dir.join("App-generated.as")).unwrap();
    let source_name = expected.lines().find_map(|l| l.strip_prefix(" *  Source:     ")).unwrap().to_string();
    let (_api, tt) = type_table(&dir);

    let file = VirtualFile::InMemory { bytes: Rc::new(Vec::new()), name: source_name.clone(), mime_type: "text/mxml", last_modified: 0 };
    let source = Source::new(file, None, "", "App", Owner::SourceList, false, true, true);
    let unit = Source::new_compilation_unit(&source, None);

    let info = DocumentInfo::new(&source_name, standard_defs());
    let local = super_class.rsplit('.').next().unwrap();
    let root_node = Node::new_document(root_ns, local, 0).into_ref();
    root_node.borrow_mut().set_version(4);
    root_node.borrow_mut().set_language_namespace(Some("http://ns.adobe.com/mxml/2009"));
    info.set_root_node(root_node, 2);
    info.set_class_name("App");
    info.set_package_name("");
    info.set_qualified_super_class_name(super_class, 2);
    for l in expected.lines() {
        if let Some(name) = l.strip_prefix("import ").and_then(|n| n.strip_suffix(';')) {
            info.add_import_name(name, 1);
        }
    }
    let states: Vec<String> = expected.lines().filter_map(|l| l.trim().strip_prefix("name: \"").and_then(|n| n.strip_suffix("\","))).map(str::to_string).collect();
    for st in &states {
        info.add_state_name(st, 4);
    }

    let doc = MxmlDocument::new(&unit, &tt, &info, &MxmlConfiguration::new(&Configuration::default()));
    let root = build(&doc, &tt);
    doc.add_metadata(Script::from_text("[Frame(extraClass=\"_App_FlexInit\")]
"));
    doc.add_metadata(Script::from_text("[Frame(factoryClass=\"_App_mx_managers_SystemManager\")]
"));
    let _ = root;

    let template = velocity::get_template_with_lib("flex2/compiler/mxml/gen/ClassDef.vm", Some("flex2/compiler/mxml/gen/ClassDefLib.vm")).unwrap();
    let map = Rc::new(DualModeLineNumberMap::new(Some(&source_name), None));
    let util = VelocityUtil::new("flex2/compiler/mxml/gen/", false, Some(map));
    let (cx, util) = velocity_util::get_code_gen_context(util, Some("-".into()));
    cx.put("doc", template_values::document(&doc));
    let out = velocity_util::merge(&template, &cx, &util).unwrap();
    for d in toolkit::take_diagnostics() {
        eprintln!("{}", d.text);
    }
    std::fs::write(dir.join("App-rust.as"), &out).unwrap();
    assert_eq!(mask_time(&out), mask_time(&expected));
}

#[test]
fn class_def_of_a_spark_button_app() {
    merge_and_compare("merge1", "spark.components.Application", "library://ns.adobe.com/flex/spark", |doc, tt| {
        let app = tt.get_type("spark.components:Application").unwrap();
        let button = tt.get_type("spark.components:Button").unwrap();
        let root = Model::new_movie_clip(doc, app.clone(), None, 2);
        doc.set_root(&root);
        let content = app.get_property("mxmlContentFactory").unwrap();
        let array = Model::new_array(doc, Some(root.clone()), 3, content.get_element_type());
        let b = Model::new_movie_clip(doc, button.clone(), Some(array.clone()), 3);
        b.set_id(Some("b"), false);
        b.set_property("label", RValue::Str("Hi".into()), 3);
        b.set_event(&button.get_event("click").unwrap(), "trace('x')", 3);
        doc.add_declaration(&b, false);
        array.add_entry_model(&b);
        root.set_property_property(&content, RValue::Model(array), 3);
        root
    });
}

#[test]
fn class_def_of_an_mx_descriptor_app() {
    merge_and_compare("merge2", "mx.core.Application", "library://ns.adobe.com/flex/mx", |doc, tt| {
        let app = tt.get_type("mx.core:Application").unwrap();
        let vbox_t = tt.get_type("mx.containers:VBox").unwrap();
        let button = tt.get_type("mx.controls:Button").unwrap();
        let root = Model::new_movie_clip(doc, app, None, 2);
        doc.set_root(&root);
        let vbox = Model::new_movie_clip(doc, vbox_t, Some(root.clone()), 3);
        vbox.set_style("paddingLeft", RValue::Integer(5), 3);
        let b = Model::new_movie_clip(doc, button.clone(), Some(vbox.clone()), 4);
        b.set_property("label", RValue::Str("A".into()), 4);
        b.set_event(&button.get_event("click").unwrap(), "trace('a')", 4);
        vbox.add_child(&b);
        root.add_child(&vbox);
        root
    });
}
