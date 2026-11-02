//! Port of `flex2.compiler.as3.binding.DataBindingExtension`: after code generation, builds the watchers of the MXML
//! class's bindings and adds a `WatcherSetupUtil` source, as an AST or generated from `WatcherSetupUtil.vm`.

use std::cell::RefCell;
use std::rc::Rc;

use super::data_binding_first_pass_evaluator::DataBindingFirstPassEvaluator;
use super::data_binding_info::{DataBindingInfo, DataBindingInfoView};
use crate::asc::flex2::compiler::as3::evaluator_adapter::evaluate as adapter_evaluate;
use crate::asc::flex2::compiler::as3::extension::Extension;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::mxml::gen::velocity_util::{self, VelocityUtil};
use crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::source::{Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::{mime_mappings, QName};
use crate::asc::flex2::velocity::{self, Value};
use crate::asc::util::jhash::JHashMap;

const TEMPLATE_PATH: &str = "flex2/compiler/as3/binding/";
const DATA_BINDING_INFO_KEY: &str = "dataBindingInfo";

/// `CompilerContext.BINDING_EXPRESSIONS`.
pub const BINDING_EXPRESSIONS: &str = "BindingExpressions";
pub type BindingExpressions = RefCell<Vec<BindingExpressionRef>>;

pub struct DataBindingExtension {
    generated_output_directory: Option<String>,
    show_binding_warnings: bool,
    generate_abstract_syntax_tree: bool,
    defines: Vec<crate::asc::util::context::ConfigVar>,
}

impl DataBindingExtension {
    pub fn new(generated_output_directory: Option<String>, show_binding_warnings: bool, generate_abstract_syntax_tree: bool, defines: Vec<crate::asc::util::context::ConfigVar>) -> DataBindingExtension {
        DataBindingExtension { generated_output_directory, show_binding_warnings, generate_abstract_syntax_tree, defines }
    }

    fn create_source(&self, file_name: &str, short_name: &str, last_modified: i64, source_code: String) -> SourceRef {
        if let Some(dir) = &self.generated_output_directory {
            let path = format!("{dir}{}{file_name}", std::path::MAIN_SEPARATOR);
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&path, source_code.as_bytes()) {
                toolkit::log(CompilerMessage::error(format!("Unable to write generated file ({file_name}): {e}.")), None, -1, -1, None);
            }
        }
        let generated_file = VirtualFile::text(source_code, file_name, None, None, mime_mappings::AS, last_modified);
        Source::new(generated_file, None, "", short_name, Owner::None, false, false, false)
    }

    fn generate_watcher_setup_util(&self, unit: &UnitRef, info: &Rc<RefCell<DataBindingInfo>>) -> Option<SourceRef> {
        let template_path = format!("{TEMPLATE_PATH}{}", StandardDefs::get_standard_defs("halo").get_watcher_setup_util_template());
        let template = match velocity::get_template(&template_path) {
            Ok(t) => t,
            Err(_) => {
                toolkit::log(CompilerMessage::error(format!("Unable to load template '{template_path}'.")), None, -1, -1, None);
                return None;
            }
        };
        let class_name = info.borrow().get_watcher_setup_util_class_name().expect("NullPointerException");
        let short_name = class_name[class_name.rfind('.').map_or(0, |i| i + 1)..].to_string();
        let generated_name = format!("{}.as", class_name.replace('.', std::path::MAIN_SEPARATOR_STR));
        let util = VelocityUtil::new(TEMPLATE_PATH, false, None);
        let (vc, util) = velocity_util::get_code_gen_context(util, None);
        vc.put(DATA_BINDING_INFO_KEY, Value::Object(Rc::new(DataBindingInfoView(info.clone()))));
        let code = match velocity_util::merge(&template, &vc, &util) {
            Ok(c) => c,
            Err(e) => {
                let rel = unit.borrow().get_source().borrow().get_relative_path().to_string();
                toolkit::log(CompilerMessage::error(format!("Exception while generating from {rel}: {e}.")), None, -1, -1, None);
                return None;
            }
        };
        let last_modified = unit.borrow().get_source().borrow().get_last_modified();
        Some(self.create_source(&generated_name, &short_name, last_modified, code))
    }
}

impl Extension for DataBindingExtension {
    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        let (cx, node, binding_expressions) = {
            let u = unit.borrow();
            (u.context.get_asc_context().unwrap(), u.get_syntax_tree().unwrap(), u.context.get_attribute_as::<BindingExpressions>(BINDING_EXPRESSIONS).map(|b| b.borrow().clone()))
        };
        let list = {
            let mut first_pass = DataBindingFirstPassEvaluator::new(binding_expressions.unwrap_or_default(), symbol_table, self.show_binding_warnings);
            adapter_evaluate(&mut first_pass, &cx, &node);
            first_pass.get_data_binding_info_list().to_vec()
        };
        if list.is_empty() {
            return;
        }
        let mut extra_sources: JHashMap<QName, SourceRef> = JHashMap::new();
        for info in &list {
            let class_qname = QName::parse(&info.borrow().get_watcher_setup_util_class_name().expect("NullPointerException"));
            if self.generate_abstract_syntax_tree {
                extra_sources.insert(class_qname, super::data_binding_extension_ast::generate_watcher_setup_util_ast(unit, symbol_table, info, &self.defines));
            } else if let Some(s) = self.generate_watcher_setup_util(unit, info) {
                extra_sources.insert(class_qname, s);
            }
        }
        unit.borrow_mut().add_generated_sources(extra_sources);
    }
}
