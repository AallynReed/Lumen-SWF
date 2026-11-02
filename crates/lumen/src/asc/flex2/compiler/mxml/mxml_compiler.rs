//! Port of `flex2.compiler.mxml.MxmlCompiler`: the two-pass driver over `InterfaceCompiler` and
//! `ImplementationCompiler`. The symbol table's `TypeTable` context attribute is held here, shared by both passes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::implementation_compiler::ImplementationCompiler;
use super::interface_compiler::InterfaceCompiler;
use super::mxml_configuration::MxmlConfiguration;
use super::reflect::TypeTable;
use crate::asc::flex2::compiler::as3::as3_compiler::SubCompiler;
use crate::asc::flex2::compiler::as3::reflect::type_table as as3_type_table;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::transcoder::Transcoders;
use crate::asc::flex2::compiler::util::name_mappings::NameMappings;

const ATTR_STATE: &str = "MxmlState";
const COMPILER_NAME: &str = "mxml";

const STATE_INTERFACE_PARSED: i32 = 0;
const STATE_INTERFACE_GENERATED: i32 = 1;
const STATE_IMPLEMENTATION_PARSED: i32 = 2;
const STATE_IMPLEMENTATION_GENERATED: i32 = 3;

pub const DOCUMENT_INFO: &str = "DocumentInfo";
pub const LINE_NUMBER_MAP: &str = "LineNumberMap";
pub const DELEGATE_UNIT: &str = "DelegateUnit";
/// Keeps the delegate unit's `Source` alive, as Java's unit references it.
pub const DELEGATE_SOURCE: &str = "DelegateSource";
pub const CHECK_NODES: &str = "CheckNodes";

/// `ThreadLocalToolkit.setLogger(adapter)` for the length of a delegated phase; dropping it restores the original.
pub struct LogAdapterGuard(pub Option<Rc<dyn crate::asc::flex2::compiler::toolkit::LogAdapter>>);

impl Drop for LogAdapterGuard {
    fn drop(&mut self) {
        crate::asc::flex2::compiler::toolkit::set_log_adapter(self.0.take());
    }
}

/// `source.getSourceFragmentLineMaps()` as the adapter's maps.
pub fn fragment_line_maps(source: &SourceRef) -> Option<Vec<super::mxml_log_adapter::LineMap>> {
    let maps = source.borrow().get_source_fragment_line_maps()?;
    Some(maps.into_iter().filter_map(|m| m.downcast::<RefCell<super::gen::line_number_map::LineNumberMap>>().ok()).map(super::mxml_log_adapter::LineMap::Plain).collect())
}

/// Installs `new MxmlLogAdapter(original, map)` with the extra maps and renamed variables.
pub fn set_mxml_log_adapter(map: Option<super::mxml_log_adapter::LineMap>, extras: Option<Vec<super::mxml_log_adapter::LineMap>>, renamed: Option<crate::asc::util::jhash::JHashMap<String, String>>) -> LogAdapterGuard {
    let original = crate::asc::flex2::compiler::toolkit::get_log_adapter();
    let adapter = super::mxml_log_adapter::MxmlLogAdapter::new(original.clone(), map);
    adapter.add_line_number_maps(extras);
    if renamed.is_some() {
        adapter.set_renamed_variable_map(renamed);
    }
    crate::asc::flex2::compiler::toolkit::set_log_adapter(Some(Rc::new(adapter)));
    LogAdapterGuard(original)
}

/// The unit's `LINE_NUMBER_MAP` attribute.
pub fn line_number_map(unit: &UnitRef) -> Option<super::mxml_log_adapter::LineMap> {
    unit.borrow().context.get_attribute_as::<super::mxml_log_adapter::LineMap>(LINE_NUMBER_MAP).map(|m| (*m).clone())
}

/// `symbolTable.getContext().getAttribute(MxmlCompiler.TYPE_TABLE)`.
pub type TypeTableSlot = Rc<RefCell<Option<Rc<TypeTable>>>>;

pub struct MxmlCompiler {
    intfc: InterfaceCompiler,
    implc: ImplementationCompiler,
}

impl MxmlCompiler {
    pub fn new(configuration: Rc<Configuration>, mappings: Rc<NameMappings>, transcoders: Transcoders) -> MxmlCompiler {
        let mxml_configuration = Rc::new(MxmlConfiguration::new(&configuration));
        let type_table: TypeTableSlot = Rc::new(RefCell::new(None));
        MxmlCompiler {
            intfc: InterfaceCompiler::new(mxml_configuration.clone(), configuration.clone(), mappings.clone(), type_table.clone(), false),
            implc: ImplementationCompiler::new(mxml_configuration, configuration, mappings, transcoders, type_table, false),
        }
    }

    fn get_state(unit: &UnitRef) -> i32 {
        unit.borrow().context.get_attribute_as::<Cell<i32>>(ATTR_STATE).expect("unit lacks MxmlState attribute").get()
    }

    fn set_state(unit: &UnitRef, state: i32) {
        unit.borrow_mut().context.set_attribute(ATTR_STATE, Rc::new(Cell::new(state)));
    }

    fn advance_state(unit: &UnitRef) {
        let state = Self::get_state(unit);
        debug_assert!(state < STATE_IMPLEMENTATION_GENERATED);
        Self::set_state(unit, state + 1);
    }

    fn sub_compiler(&mut self, unit: &UnitRef) -> &mut dyn SubCompiler {
        if Self::get_state(unit) < STATE_IMPLEMENTATION_PARSED {
            &mut self.intfc
        } else {
            &mut self.implc
        }
    }
}

/// `MxmlCompiler.getGeneratedName(mxmlConfiguration, packageName, className, suffix)`.
pub fn get_generated_name(mxml_configuration: &MxmlConfiguration, package_name: &str, class_name: &str, suffix: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let mut dir = mxml_configuration.get_generated_directory().map(str::to_string);
    if !package_name.is_empty() {
        dir = Some(add_path_components(dir.as_deref(), &package_name.replace('.', &sep.to_string()), sep));
    }
    add_path_components(dir.as_deref(), &format!("{class_name}{suffix}"), sep)
}

/// `FileUtils.addPathComponents(p1, p2, sepChar)`.
pub fn add_path_components(p1: Option<&str>, p2: &str, sep: char) -> String {
    let p1 = p1.unwrap_or("");
    let l1 = p1.len();
    if l1 == 0 {
        return p2.to_string();
    }
    let ends = p1.ends_with(sep);
    let starts = p2.starts_with(sep);
    if ends && starts {
        format!("{}{}", p1, &p2[1..])
    } else if !ends && !starts {
        format!("{p1}{sep}{p2}")
    } else {
        format!("{p1}{p2}")
    }
}

impl SubCompiler for MxmlCompiler {
    fn get_name(&self) -> &'static str {
        COMPILER_NAME
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        self.implc.is_supported(mime_type)
    }

    fn preprocess(&mut self, source: SourceRef) -> Option<SourceRef> {
        if source.borrow().get_compilation_unit().is_none() {
            self.intfc.preprocess(source)
        } else {
            self.implc.preprocess(source)
        }
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let unit = source.borrow().get_compilation_unit();
        if let Some(u) = &unit {
            if u.borrow().has_type_info {
                return unit;
            }
        }
        match unit {
            None => {
                let unit = self.intfc.parse1(source, symbol_table);
                if let Some(u) = &unit {
                    Self::set_state(u, STATE_INTERFACE_PARSED);
                }
                unit
            }
            Some(u) => {
                if Self::get_state(&u) == STATE_INTERFACE_GENERATED {
                    let unit = self.implc.parse1(source, symbol_table);
                    if let Some(u) = &unit {
                        Self::advance_state(u);
                    }
                    unit
                } else {
                    Some(u)
                }
            }
        }
    }

    fn parse2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).parse2(unit, symbol_table);
    }

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).analyze1(unit, symbol_table);
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).analyze2(unit, symbol_table);
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).analyze3(unit, symbol_table);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            let entries: Vec<(String, Rc<crate::asc::flex2::compiler::as3::reflect::as3_class::AbcClass>)> = unit.borrow().class_table.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let type_table = as3_type_table::TypeTable::new(symbol_table);
            for (name, c) in entries {
                c.set_type_table(type_table.clone());
                symbol_table.register_class(&name, c);
            }
            crate::asc::flex2::compiler::as3::style_evaluator::register_unit_styles(unit, symbol_table);
            crate::asc::flex2::compiler::as3::as3_compiler::evaluate_loader_class_base(unit, symbol_table);
            return;
        }
        self.sub_compiler(unit).analyze4(unit, symbol_table);
    }

    fn generate(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).generate(unit, symbol_table);
        Self::advance_state(unit);
    }

    fn postprocess(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        self.sub_compiler(unit).postprocess(unit, symbol_table);
    }
}

