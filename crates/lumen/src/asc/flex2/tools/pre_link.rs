//! Port of `flex2.tools.PreLink`: `run` (`processMainUnit`: main definition, root class, `[SWF]` metadata and the
//! styles) and `postRun` (`_<App>_FlexInit`, the `[Frame(factoryClass)]` loader subclass such as
//! `_<App>_mx_managers_SystemManager`, `_CompiledResourceBundleInfo`, then the root's styles). The style work is
//! `pre_link_css`'s. Not ported: `IPreLinkExtension`s, services (`-services`) and the SWC-version check
//! (`DependencyNotCompatible`).

use std::rc::Rc;

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::compiler_api::CompilerApi;
use crate::asc::flex2::compiler::configuration::Configuration;
use crate::asc::flex2::compiler::flex_options::{FrameInfo, RslPathInfo};
use crate::asc::flex2::compiler::i18n::i18n_utils;
use crate::asc::flex2::compiler::source::{Owner, Source, SourceRef, VirtualFile};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::flex2::flash::swf::tags::TagRef;
use crate::asc::util::jhash::JHashSet;
use crate::asc::util::jstr;

/// `System.getProperty("line.separator")`.
const LINE_SEP: &str = if cfg!(windows) { "\r\n" } else { "\n" };

const INTERFACE_IFLEXMODULEFACTORY_DOT: &str = "mx.core.IFlexModuleFactory";
const INTERFACE_IFLEXMODULE_DOT: &str = "mx.core.IFlexModule";
const INTERFACE_ISTYLEMANAGER2_DOT: &str = "mx.styles.IStyleManager2";
const CLASS_REQUEST_DOT: &str = "mx.events.Request";
const CLASS_STYLEMANAGERIMPL_DOT: &str = "mx.styles.StyleManagerImpl";
const CLASS_STYLEMANAGER_DOT: &str = "mx.styles.StyleManager";
const CLASS_SYSTEMMANAGERCHILDMANAGER_DOT: &str = "mx.managers.systemClasses.ChildManager";
const CLASS_TEXTFIELDFACTORY_DOT: &str = "mx.core.TextFieldFactory";
const CLASS_EFFECTMANAGER_DOT: &str = "mx.effects.EffectManager";
const NAMESPACE_MX_INTERNAL_DOT: &str = "mx.core.mx_internal";
const CLASS_CROSSDOMAINRSLITEM_DOT: &str = "mx.core.CrossDomainRSLItem";
const CLASS_EMBEDDEDFONTREGISTRY_DOT: &str = "mx.core.EmbeddedFontRegistry";
const CLASS_SINGLETON_DOT: &str = "mx.core.Singleton";
const CLASS_FLEXVERSION_DOT: &str = "mx.core.FlexVersion";

const VERSION_3_0: i32 = 0x03000000;
const VERSION_4_0: i32 = 0x04000000;
const CURRENT_VERSION: i32 = 0x04060000;

/// `Integer.decode`.
pub fn integer_decode(s: &str) -> Option<i32> {
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

/// The non-style half of `processMainUnit`; `None` when there is no root.
fn process_main_unit(units: &[UnitRef], configuration: &mut Configuration) {
    let Some(u) = units.iter().find(|u| u.borrow().is_root()) else { return };
    swfmetadata(u, configuration);
    let loader = u.borrow().loader_class.clone();
    if let Some(loader) = loader {
        configuration.root_class_name = Some(loader);
    }
    let last = u.borrow().top_level_definitions.last().map(|q| q.to_string());
    match last {
        Some(def) => {
            configuration.main_definition = Some(def.clone());
            u.borrow_mut().context.main_definition = Some(def);
        }
        None => {
            let name = u.borrow().get_source().borrow().get_name_for_reporting().to_string();
            toolkit::log_for(CompilerMessage::error("The target file does not have an externally visible definition."), &name);
        }
    }
}

/// `run(sources, units, ...)`: true when the batch must be compiled and `run` called again.
pub fn run(api: &mut CompilerApi) -> bool {
    let initial_source_count = api.sources.len();
    let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
    process_main_unit(&units, &mut api.configuration);
    let re_run_prelink = super::pre_link_css::process_main_unit_styles(api);
    api.sources.len() > initial_source_count || re_run_prelink
}

/// The compile loop's `PreLink` hook: `run`, then `postRun` once it returns false.
pub fn pre_link(api: &mut CompilerApi) -> bool {
    let rerun = run(api);
    if !rerun {
        post_run(api);
    }
    rerun
}

/// `TreeSet<String>` with `String.compareTo` order.
#[derive(Default, Clone)]
struct SortedSet(Vec<String>);

impl SortedSet {
    fn add(&mut self, s: &str) {
        if let Err(i) = self.0.binary_search_by(|x| jstr::compare(x, s)) {
            self.0.insert(i, s.to_string());
        }
    }
}

/// `TreeMap<String, V>` with `String.compareTo` order.
struct SortedMap<V>(Vec<(String, V)>);

impl<V> SortedMap<V> {
    fn new() -> Self {
        SortedMap(Vec::new())
    }

    fn put(&mut self, k: &str, v: V) -> Option<V> {
        match self.0.binary_search_by(|x| jstr::compare(&x.0, k)) {
            Ok(i) => Some(std::mem::replace(&mut self.0[i].1, v)),
            Err(i) => {
                self.0.insert(i, (k.to_string(), v));
                None
            }
        }
    }
}

struct State {
    resource_bundle_names: SortedSet,
    external_resource_bundle_names: SortedSet,
}

/// `postRun(sources, units, resources, symbolTable, swcContext, nameMappings, configuration)`.
pub fn post_run(api: &mut CompilerApi) {
    let units: Vec<UnitRef> = api.units.iter().flatten().cloned().collect();
    let mut extra_sources: Vec<SourceRef> = Vec::new();
    let mut mixins: Vec<String> = Vec::new();
    let mut fonts: Vec<TagRef> = Vec::new();
    let mut contributing_swcs: JHashSet<String> = JHashSet::new();
    let mut state = State { resource_bundle_names: SortedSet::default(), external_resource_bundle_names: SortedSet::default() };
    process_resource_bundle_names(&mut state, &units, &api.configuration);
    let externs: JHashSet<String> = {
        let mut s = JHashSet::new();
        for e in api.swc_context.get_externs() {
            s.insert(e);
        }
        s
    };
    process_init_class(&state, &units, &api.configuration, &mut extra_sources, &mut mixins, &mut fonts, &mut contributing_swcs, &externs);
    let generated_loader_class = process_loader_class(&state, &units, &api.configuration, &mut extra_sources, &mixins, &fonts);
    if !generated_loader_class {
        process_compiled_resource_bundle_info_class(&state, &mut api.configuration, &mut extra_sources);
    }
    for s in extra_sources {
        s.borrow_mut().set_owner(Owner::ResourceContainer);
        api.sources.push(s);
    }
    let check = |api: &CompilerApi, range: &[UnitRef]| {
        if generated_loader_class {
            for u in range {
                check_for_module_or_application(api, u);
            }
        }
    };
    match units.iter().position(|u| u.borrow().is_root()) {
        Some(r) => {
            check(api, &units[..r]);
            super::pre_link_css::post_run_styles(api);
            check(api, &units[r + 1..]);
        }
        None => check(api, &units),
    }
    if let Some(u) = units.iter().find(|u| u.borrow().is_root()) {
        u.borrow().get_source().borrow_mut().disconnect_logger();
    }
}

/// The non-root branch of `postRun`'s unit loop: `checkForModuleOrApplication` for each top-level definition.
fn check_for_module_or_application(api: &CompilerApi, unit: &UnitRef) {
    use crate::asc::flex2::compiler::mxml::lang::standard_defs::{StandardDefs, CLASS_APPLICATION, CLASS_SPARK_APPLICATION, INTERFACE_IMODULE_NO_PACKAGE};
    let (skip, defs) = {
        let u = unit.borrow();
        let s = u.get_source();
        let skip = s.borrow().is_internal() || s.borrow().is_swc_script_owner();
        (skip, u.top_level_definitions.0.clone())
    };
    if skip {
        return;
    }
    let type_analyzer = api.symbol_table.get_type_analyzer();
    let standard_defs = StandardDefs::get_standard_defs("halo");
    for q_name in defs {
        let Some(info) = type_analyzer.borrow().get_class_info(&q_name.to_string()) else { continue };
        let is_module_or_application = info.borrow().implements_interface(standard_defs.get_modules_package(), INTERFACE_IMODULE_NO_PACKAGE)
            || info.borrow().extends_class(CLASS_APPLICATION)
            || info.borrow().extends_class(CLASS_SPARK_APPLICATION);
        if !is_module_or_application {
            continue;
        }
        let main_definition = api.configuration.main_definition.clone().unwrap_or_default();
        let Some(root_info) = type_analyzer.borrow().get_class_info(&main_definition) else { continue };
        if !root_info.borrow().implements_interface(q_name.get_namespace(), q_name.get_local_part()) && !root_info.borrow().extends_class(&q_name.to_string()) {
            let class_name = q_name.to_string();
            let text = format!("{class_name} is a module or application that is directly referenced. This will cause {class_name} and all of its dependencies to be linked in with {main_definition}. Using an interface is the recommended practice to avoid this.");
            toolkit::log(CompilerMessage::warning(text), None, -1, -1, None);
        }
    }
}

/// `processResourceBundleNames(units, configuration)`.
fn process_resource_bundle_names(state: &mut State, units: &[UnitRef], configuration: &Configuration) {
    for unit in units {
        let u = unit.borrow();
        if !u.resource_bundle_history.is_empty() {
            for rb in u.resource_bundle_history.iter() {
                state.resource_bundle_names.add(rb);
            }
            let first = u.top_level_definitions.first().map(|q| q.to_string()).expect("NullPointerException");
            if configuration.externs.contains(&first) {
                for rb in u.resource_bundle_history.iter() {
                    state.external_resource_bundle_names.add(rb);
                }
            }
        }
    }
}

/// `codegenFlexInit(flexInitClassName, accessibilityList, remoteClassAliases, effectTriggers, inheritingStyles, configuration)`.
fn codegen_flex_init(state: &State, flex_init_class_name: &str, accessibility_list: Option<&JHashSet<String>>, remote_class_aliases: &SortedMap<String>, effect_triggers: &SortedMap<String>, inheriting_styles: &[String], configuration: &Configuration) -> String {
    let mut sb = String::new();
    sb.push_str("package {\n");
    sb.push_str("import flash.display.DisplayObject;\n");
    sb.push_str("import flash.utils.*;\n");
    sb.push_str(&format!("import {INTERFACE_IFLEXMODULEFACTORY_DOT};\n"));
    sb.push_str(&format!("import {INTERFACE_ISTYLEMANAGER2_DOT};\n"));
    sb.push_str(&format!("import {CLASS_REQUEST_DOT};\n"));
    sb.push_str(&format!("import {CLASS_STYLEMANAGERIMPL_DOT};\n"));
    sb.push_str(&format!("import {CLASS_SYSTEMMANAGERCHILDMANAGER_DOT};\n"));
    sb.push_str(&format!("import {CLASS_TEXTFIELDFACTORY_DOT}; TextFieldFactory;\n"));
    sb.push_str(&codegen_accessibility_imports(accessibility_list));
    sb.push_str(&codegen_remote_class_imports(remote_class_aliases));
    sb.push_str(&codegen_effect_trigger_imports(effect_triggers));
    sb.push_str(&codegen_resource_bundle_metadata(Some(&state.external_resource_bundle_names)));
    sb.push_str("\n[Mixin]\n");
    sb.push_str(&format!("public class {flex_init_class_name}\n"));
    sb.push_str("{\n");
    sb.push_str(&format!("   public function {flex_init_class_name}()\n"));
    sb.push_str("   {\n");
    sb.push_str("       super();\n");
    sb.push_str("   }\n");
    sb.push_str("   public static function init(fbs:IFlexModuleFactory):void\n");
    sb.push_str("   {\n");
    sb.push_str("       new ChildManager(fbs);\n");
    sb.push_str("       var styleManager:IStyleManager2;\n");
    if configuration.css.compatibility_version() <= VERSION_3_0 {
        sb.push_str("       styleManager = StyleManagerImpl.getInstance();\n");
        sb.push_str("       fbs.registerImplementation(\"mx.styles::IStyleManager2\", styleManager);\n");
    } else if !configuration.css.isolate_styles {
        sb.push_str("       var request:mx.events.Request = new mx.events.Request(mx.events.Request.GET_PARENT_FLEX_MODULE_FACTORY_REQUEST);\n");
        sb.push_str("       DisplayObject(fbs).dispatchEvent(request);\n");
        sb.push_str("       var moduleFactory:IFlexModuleFactory = request.value as IFlexModuleFactory;\n");
        sb.push_str("       if (moduleFactory)\n");
        sb.push_str("           styleManager = IStyleManager2(moduleFactory.getImplementation(\"mx.styles::IStyleManager2\"));\n\n");
        sb.push_str("       if (!styleManager)\n");
        sb.push_str("           styleManager = StyleManagerImpl.getInstance();\n");
        sb.push_str("       fbs.registerImplementation(\"mx.styles::IStyleManager2\", styleManager);\n");
    } else {
        sb.push_str("       styleManager = new StyleManagerImpl(fbs);\n");
    }
    sb.push_str(super::pre_link_css::codegen_qualified_type_selectors(configuration.css.qualified_type_selectors));
    sb.push_str(&codegen_effect_trigger_registration(effect_triggers));
    sb.push_str(&codegen_accessibility_list(accessibility_list));
    sb.push_str(&codegen_remote_class_aliases(remote_class_aliases, flex_init_class_name, configuration));
    sb.push_str(&super::pre_link_css::codegen_inheriting_style_registration(inheriting_styles));
    sb.push_str("   }\n");
    sb.push_str("}  // FlexInit\n");
    sb.push_str("}  // package\n");
    sb
}

/// `isCompilationUnitExternal(unit, externs)`.
pub fn is_compilation_unit_external(unit: &UnitRef, externs: &JHashSet<String>) -> bool {
    unit.borrow().top_level_definitions.iter().any(|q| externs.contains(&q.to_string()))
}

/// The `TreeMap.put` override of `processInitClass`'s `remoteClassAliases`, warning about shared aliases.
fn put_remote_class_alias(map: &mut SortedMap<String>, key: &str, value: &str) {
    if let Some((existing_key, _)) = map.0.iter().find(|(_, v)| v == value) {
        let existing_key = existing_key.clone();
        toolkit::log(
            CompilerMessage::warning(format!("A mapping for '{existing_key}' to the remote class alias '{value}' was found while processing class '{key}'. Flex now generates code to check if an alias has already been registered. When deserializing remote data an alias can only map to a single class.")),
            None,
            -1,
            -1,
            None,
        );
    }
    map.put(key, value.to_string());
}

/// `processInitClass(units, configuration, extraSources, mixins, fonts, contributingSwcs, swcContext)`.
#[allow(clippy::too_many_arguments)]
fn process_init_class(state: &State, units: &[UnitRef], configuration: &Configuration, extra_sources: &mut Vec<SourceRef>, mixins: &mut Vec<String>, fonts: &mut Vec<TagRef>, contributing_swcs: &mut JHashSet<String>, externs: &JHashSet<String>) {
    let mut accessibility_list: Option<JHashSet<String>> = None;
    let mut remote_class_aliases: SortedMap<String> = SortedMap::new();
    let mut effect_triggers: SortedMap<String> = SortedMap::new();
    let mut inheriting_styles: JHashSet<String> = JHashSet::new();
    let mut main_unit = None;
    let remove_unused_rsls = configuration.flex.remove_unused_rsls;
    for unit in units {
        let u = unit.borrow();
        if u.is_root() {
            main_unit = Some(unit.clone());
        }
        if u.has_assets() {
            let font_list = u.assets.get_fonts();
            let internal = u.get_source().borrow().is_internal();
            if !font_list.is_empty() && !is_compilation_unit_external(unit, externs) && !internal {
                fonts.extend(font_list);
            }
        }
        let mut aliases: Vec<(&String, &String)> = u.remote_class_aliases.iter().collect();
        aliases.sort_by(|a, b| jstr::compare(a.0, b.0));
        for (k, v) in aliases {
            put_remote_class_alias(&mut remote_class_aliases, k, v);
        }
        let mut triggers: Vec<(&String, &String)> = u.effect_triggers.iter().collect();
        triggers.sort_by(|a, b| jstr::compare(a.0, b.0));
        for (k, v) in triggers {
            effect_triggers.put(k, v.clone());
        }
        mixins.extend(u.mixins.iter().cloned());
        for s in u.styles.get_inheriting_styles().iter() {
            inheriting_styles.insert(s.clone());
        }
        if configuration.flex.accessible {
            if let Some(a) = &u.accessibility_classes {
                let list = accessibility_list.get_or_insert_with(JHashSet::new);
                for c in a.iter() {
                    list.insert(c.clone());
                }
            }
        }
        if remove_unused_rsls {
            let s = u.get_source();
            let s = s.borrow();
            if !s.is_internal() {
                if let Some(script) = s.swc_script() {
                    contributing_swcs.insert(script.get_swc_location());
                }
            }
        }
    }
    let mut flex_init_class = None;
    if let Some(m) = &main_unit {
        flex_init_class = m.borrow().extra_classes.iter().find(|c| c.contains("FlexInit")).cloned();
    }
    if let Some(flex_init_class) = flex_init_class {
        let styles: Vec<String> = inheriting_styles.iter().cloned().collect();
        let code = codegen_flex_init(state, &flex_init_class, accessibility_list.as_ref(), &remote_class_aliases, &effect_triggers, &styles, configuration);
        let name = format!("{flex_init_class}-generated.as");
        if let Some(dir) = &configuration.generated_directory {
            save_generated(&name, &code, dir);
        }
        let file = VirtualFile::InMemory { bytes: Rc::new(code.into_bytes()), name, mime_type: mime_mappings::AS, last_modified: now() };
        let s = Source::new(file, None, "", &flex_init_class, Owner::None, false, false, false);
        extra_sources.push(s);
        mixins.insert(0, flex_init_class);
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `processLoaderClass(units, configuration, sources, mixins, fonts, contributingSwcs, swcContext)`.
fn process_loader_class(state: &State, units: &[UnitRef], configuration: &Configuration, sources: &mut Vec<SourceRef>, mixins: &[String], fonts: &[TagRef]) -> bool {
    if !configuration.generate_frame_loader {
        return false;
    }
    let frames: Vec<FrameInfo> = configuration.flex.frame_list.clone();
    let Some(main_unit) = units.iter().find(|u| u.borrow().is_root()).cloned() else { return false };
    let cd_rsls: &[RslPathInfo] = &configuration.flex.rsl_path_info;
    let rsls: &[String] = &configuration.flex.runtime_shared_libraries;
    let locales: &[String] = &configuration.flex.locales;
    let (loader_class, loader_class_base, aux) = {
        let m = main_unit.borrow();
        (m.loader_class.clone(), m.loader_class_base.clone(), m.aux_generate_info.clone())
    };
    let (generate_loader_class, base_loader_class, window_class, root_attributes, root_attribute_embed_vars, root_attribute_embed_names);
    if let (Some(_), Some(aux)) = (&loader_class, &aux) {
        generate_loader_class = aux.generate_loader_class.clone();
        base_loader_class = aux.base_loader_class.clone();
        window_class = aux.window_class.clone();
        root_attributes = aux.root_attributes.clone();
        root_attribute_embed_vars = aux.root_attribute_embed_vars.clone();
        root_attribute_embed_names = aux.root_attribute_embed_names.clone();
    } else if let (None, Some(base)) = (&loader_class, &loader_class_base) {
        base_loader_class = base.clone();
        window_class = main_unit.borrow().top_level_definitions.last().map(|q| q.to_string()).expect("NullPointerException");
        generate_loader_class = format!("{window_class}_{base}").chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        main_unit.borrow_mut().loader_class = Some(generate_loader_class.clone());
        root_attributes = None;
        root_attribute_embed_vars = None;
        root_attribute_embed_names = None;
    } else if loader_class.is_none() && (!rsls.is_empty() || !cd_rsls.is_empty()) {
        let name = main_unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
        toolkit::log_for(CompilerMessage::warning("This compilation unit did not have a factoryClass specified in Frame metadata to load the configured runtime shared libraries. To compile without runtime shared libraries either set the -static-link-runtime-shared-libraries option to true or remove the -runtime-shared-libraries option."), &name);
        return false;
    } else {
        return false;
    }
    let generated_loader_code = codegen_module_factory(
        &base_loader_class.replace(':', "."),
        &generate_loader_class.replace(':', "."),
        Some(&window_class.replace(':', ".")),
        root_attributes.as_deref(),
        root_attribute_embed_vars.as_deref(),
        root_attribute_embed_names.as_deref(),
        cd_rsls,
        rsls,
        mixins,
        fonts,
        &frames,
        Some(locales),
        &state.resource_bundle_names.0,
        configuration,
        false,
    );
    let generated_loader_file = format!("{generate_loader_class}.as");
    let main_source = main_unit.borrow().get_source();
    let (path_root, relative_path) = {
        let m = main_source.borrow();
        (m.get_path_root().map(str::to_string), m.get_relative_path().to_string())
    };
    let file = VirtualFile::InMemory { bytes: Rc::new(generated_loader_code.clone().into_bytes()), name: generated_loader_file.clone(), mime_type: mime_mappings::AS, last_modified: now() };
    let s = Source::new(file, path_root, &relative_path, &generate_loader_class, Owner::None, false, false, true);
    sources.push(s);
    if let Some(dir) = &configuration.generated_directory {
        save_generated(&generated_loader_file, &generated_loader_code, dir);
    }
    true
}

fn codegen_accessibility_imports(accessibility_implementations: Option<&JHashSet<String>>) -> String {
    let mut sb = String::from("import flash.system.*\n");
    for i in accessibility_implementations.iter().flat_map(|s| s.iter()) {
        sb.push_str(&format!("import {i};\n"));
    }
    sb
}

/// `saveGenerated(name, code, dir)`.
pub fn save_generated(name: &str, code: &str, dir: &str) {
    let mut name = name.to_string();
    if !name.ends_with("-generated.as") && name.ends_with(".as") {
        name = format!("{}-generated.as", &name[..name.len() - 3]);
    }
    let path = std::path::Path::new(dir).join(&name);
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    if let Err(e) = std::fs::write(&path, code) {
        toolkit::log(CompilerMessage::error(format!("Unable to write generated file {}: {e}", path.display())), None, -1, -1, None);
    }
}

fn quoted_list(items: &[String]) -> String {
    items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
}

/// `codegenRslList(rsls)`.
fn codegen_rsl_list(rsls: &[String]) -> String {
    if rsls.is_empty() {
        return "[]".to_string();
    }
    let mut rb = String::from("[");
    for (i, rsl_url) in rsls.iter().enumerate() {
        toolkit::log_plain(Level::Info, &format!("    {rsl_url}"));
        rb.push_str(&format!("{{url: \"{rsl_url}\", size: -1}}"));
        if i + 1 < rsls.len() {
            rb.push_str(", ");
        }
    }
    rb.push_str("]\n");
    rb
}

/// `codegenMixinList(mixins)`.
fn codegen_mixin_list(mixins: &[String]) -> String {
    format!("[ {} ]", quoted_list(mixins))
}

/// `codegenFrameClassList(frames)`.
fn codegen_frame_class_list(frames: &[FrameInfo]) -> String {
    let items: Vec<String> = frames.iter().map(|f| format!("\"{}\":\"{}\"", f.label, f.frame_classes.first().cloned().unwrap_or_default())).collect();
    format!("{{{}}}\n", items.join(", "))
}

/// `codegenFontList(fonts)`.
fn codegen_font_list(fonts: &[TagRef]) -> String {
    if fonts.is_empty() {
        return String::new();
    }
    #[derive(Default)]
    struct FontInfo {
        plain: bool,
        bold: bool,
        italic: bool,
        bolditalic: bool,
    }
    let mut font_map: SortedMap<FontInfo> = SortedMap::new();
    for font in fonts {
        let f = font.borrow();
        let name = f.get_font_name().unwrap_or_else(|| "null".to_string());
        if !font_map.0.iter().any(|(k, _)| *k == name) {
            font_map.put(&name, FontInfo::default());
        }
        let fi = &mut font_map.0.iter_mut().find(|(k, _)| *k == name).unwrap().1;
        let (b, i) = (f.is_bold(), f.is_italic());
        fi.plain |= !b && !i;
        fi.bolditalic |= b && i;
        fi.bold |= b;
        fi.italic |= i;
    }
    let mut sb = String::from("      {\n");
    let n = font_map.0.len();
    for (j, (font_name, fi)) in font_map.0.iter().enumerate() {
        sb.push_str(&format!("\"{font_name}\" : {{regular:{}, bold:{}, italic:{}, boldItalic:{}}}\n", fi.plain, fi.bold, fi.italic, fi.bolditalic));
        if j + 1 < n {
            sb.push_str(",\n");
        }
    }
    sb.push_str("}\n");
    sb
}

fn codegen_accessibility_list(accessibility_implementations: Option<&JHashSet<String>>) -> String {
    let Some(list) = accessibility_implementations.filter(|l| !l.is_empty()) else { return String::new() };
    let mut sb = String::new();
    sb.push_str("       // trace(\"Flex accessibility startup: \" + Capabilities.hasAccessibility);\n");
    sb.push_str("       if (Capabilities.hasAccessibility) {\n");
    for i in list.iter() {
        sb.push_str(&format!("          {i}.enableAccessibility();\n"));
    }
    sb.push_str("       }\n");
    sb
}

fn codegen_remote_class_imports(remote_class_aliases: &SortedMap<String>) -> String {
    let mut sb = String::new();
    if !remote_class_aliases.0.is_empty() {
        sb.push_str("import flash.net.registerClassAlias;\nimport flash.net.getClassByAlias;\n");
        sb.push_str("import mx.managers.SystemManagerGlobals;\n");
        sb.push_str("import mx.resources.ResourceManager;\n");
    }
    for (class_name, _) in &remote_class_aliases.0 {
        sb.push_str(&format!("import {class_name};\n"));
    }
    sb
}

fn codegen_remote_class_aliases(remote_class_aliases: &SortedMap<String>, init_class_name: &str, configuration: &Configuration) -> String {
    let mut sb = String::new();
    let main = configuration.main_definition.clone().unwrap_or_else(|| "null".to_string());
    for (class_name, alias) in &remote_class_aliases.0 {
        sb.push_str(&format!("       // {class_name}\n"));
        sb.push_str("       try \n");
        sb.push_str("       { \n");
        sb.push_str(&format!("           if (flash.net.getClassByAlias(\"{alias}\") != {class_name}) \n"));
        sb.push_str("           { \n");
        sb.push_str(&format!("               flash.net.registerClassAlias(\"{alias}\", {class_name}); \n"));
        if configuration.debug {
            sb.push_str("               if (fbs != SystemManagerGlobals.topLevelSystemManagers[0]) \n");
            sb.push_str("               { \n");
            sb.push_str("                   trace(ResourceManager.getInstance().getString( \"core\", \n");
            sb.push_str("                         \"remoteClassMemoryLeak\",\n");
            sb.push_str(&format!("                         [\"{class_name}\",\"{main}\",\"{init_class_name}\"]));\n"));
            sb.push_str("               } \n");
        }
        sb.push_str("           } \n");
        sb.push_str("       } \n");
        sb.push_str("       catch (e:Error) \n");
        sb.push_str("       { \n");
        sb.push_str(&format!("           flash.net.registerClassAlias(\"{alias}\", {class_name}); \n"));
        if configuration.debug {
            sb.push_str("           if (fbs != SystemManagerGlobals.topLevelSystemManagers[0]) \n");
            sb.push_str("           { \n");
            sb.push_str("               trace(ResourceManager.getInstance().getString( \"core\", \n");
            sb.push_str("                     \"remoteClassMemoryLeak\",\n");
            sb.push_str(&format!("                     [\"{class_name}\",\"{main}\",\"{init_class_name}\"]));\n"));
            sb.push_str("           } \n");
        }
        sb.push_str("       } \n\n");
    }
    sb
}

fn codegen_effect_trigger_imports(effect_triggers: &SortedMap<String>) -> String {
    if effect_triggers.0.is_empty() {
        String::new()
    } else {
        format!("import {CLASS_EFFECTMANAGER_DOT};\nimport {NAMESPACE_MX_INTERNAL_DOT};\n")
    }
}

fn codegen_effect_trigger_registration(effect_triggers: &SortedMap<String>) -> String {
    let mut sb = String::new();
    for (name, event) in &effect_triggers.0 {
        sb.push_str(&format!("       EffectManager.mx_internal::registerEffectTrigger(\"{name}\", \"{event}\");\n"));
    }
    sb
}

/// `codegenCompatibilityCall(configuration)`.
fn codegen_compatibility_call(configuration: &Configuration) -> String {
    match configuration.css.compatibility_version_string() {
        Some(s) if configuration.css.compatibility_version() != CURRENT_VERSION => format!("        FlexVersion.compatibilityVersionString = \"{s}\";"),
        _ => String::new(),
    }
}

/// `codegenModuleFactory(base, rootClassName, topLevelWindowClass, ...)`.
#[allow(clippy::too_many_arguments)]
pub fn codegen_module_factory(
    base: &str,
    root_class_name: &str,
    top_level_window_class: Option<&str>,
    root_attributes: Option<&[(String, String)]>,
    root_attribute_embed_vars: Option<&[(String, String)]>,
    root_attribute_embed_names: Option<&[(String, String)]>,
    cd_rsls: &[RslPathInfo],
    rsls: &[String],
    mixins: &[String],
    fonts: &[TagRef],
    frames: &[FrameInfo],
    locales: Option<&[String]>,
    resource_bundle_names: &[String],
    configuration: &Configuration,
    is_library_compile: bool,
) -> String {
    let l = LINE_SEP;
    let has_fonts = !fonts.is_empty();
    let ov = |s: &'static str| if !is_library_compile { s } else { "" };
    [
        "package", l, "{", l, l,
        &codegen_imports(base, root_attributes, root_attribute_embed_vars, fonts, configuration, is_library_compile, has_fonts),
        &codegen_resource_bundle_metadata(None),
        "[ExcludeClass]", l,
        "public class ", root_class_name, l,
        "    extends ", base, l,
        "    implements IFlexModuleFactory, ISWFContext", l,
        "{", l,
        &codegen_link_in_cross_domain_rsl_item(cd_rsls),
        "    public function ", root_class_name, "()", l,
        "    {", l,
        &codegen_compatibility_call(configuration), l,
        "        super();", l,
        &codegen_add_rsl_complete_listener(is_library_compile, has_fonts),
        "    }", l, l,
        ov("    override "),
        "    public function callInContext(fn:Function, thisArg:Object, argArray:Array, returns:Boolean=true):*", l,
        "    {", l,
        "        if (returns)", l,
        "           return fn.apply(thisArg, argArray);", l,
        "        else", l,
        "           fn.apply(thisArg, argArray);", l,
        "    }", l, l,
        &codegen_add_preloaded_rsl_stub(is_library_compile),
        &codegen_get_register_implementation_stubs(is_library_compile),
        ov("    override "),
        "    public function create(... params):Object", l,
        "    {", l,
        &codegen_create_apply(is_library_compile),
        &codegen_get_main_class_name(top_level_window_class),
        "        var mainClass:Class = Class(getDefinitionByName(mainClassName));", l,
        "        if (!mainClass)", l,
        "            return null;", l, l,
        "        var instance:Object = new mainClass();", l,
        "        if (instance is IFlexModule)", l,
        "            (IFlexModule(instance)).moduleFactory = this;", l,
        &codegen_register_embedded_fonts(fonts),
        "        return instance;", l,
        "    }", l, l,
        &codegen_embedded_applicaiton_attributes(root_attribute_embed_vars),
        "    /**", l,
        "     *  @private", l,
        "     */", l,
        "    private var _info:Object;", l, l,
        ov("    override"),
        "    public function info():Object", l,
        "    {", l,
        "        if (!_info)", l,
        "        {", l,
        "            _info = {", l,
        &codegen_info(top_level_window_class, root_attributes, root_attribute_embed_names, cd_rsls, rsls, mixins, fonts, frames, locales, resource_bundle_names, configuration),
        "            }", l,
        "        }", l,
        "        return _info;", l,
        "    }", l, l,
        &codegen_rsl_security_wrapper(is_library_compile), l,
        &codegen_module_factory_security_wrapper(is_library_compile, has_fonts), l,
        &codegen_rsl_complete_listener(is_library_compile, has_fonts),
        "}", l, l,
        "}", l,
    ]
    .concat()
}

fn codegen_link_in_cross_domain_rsl_item(cd_rsls: &[RslPathInfo]) -> String {
    if cd_rsls.is_empty() {
        return String::new();
    }
    let l = LINE_SEP;
    format!("    // Cause the CrossDomainRSLItem class to be linked into this application.{l}    import {CLASS_CROSSDOMAINRSLITEM_DOT}; CrossDomainRSLItem;{l}{l}")
}

fn codegen_import_embedded_font_registry(fonts: &[TagRef]) -> String {
    if fonts.is_empty() {
        return String::new();
    }
    let l = LINE_SEP;
    format!("import {CLASS_EMBEDDEDFONTREGISTRY_DOT};{l}import {CLASS_SINGLETON_DOT};{l}")
}

fn codegen_register_embedded_fonts(fonts: &[TagRef]) -> String {
    if fonts.is_empty() {
        return String::new();
    }
    let l = LINE_SEP;
    [
        "        if (params.length == 0) {", l,
        "            Singleton.registerClass(\"mx.core::IEmbeddedFontRegistry\",", l,
        "                Class(getDefinitionByName(\"mx.core::EmbeddedFontRegistry\")));", l,
        "            EmbeddedFontRegistry.registerFonts(info()[\"fonts\"], this);", l,
        "}",
    ]
    .concat()
}

fn codegen_create_apply(is_library_compile: bool) -> String {
    if is_library_compile {
        return String::new();
    }
    let l = LINE_SEP;
    format!("        if (params.length > 0 && !(params[0] is String)){l}            return super.create.apply(this, params);{l}{l}")
}

fn codegen_get_main_class_name(top_level_window_class: Option<&str>) -> String {
    let l = LINE_SEP;
    match top_level_window_class {
        None => format!("        var mainClassName:String = String(params[0]){l}"),
        Some(c) => format!("        var mainClassName:String = params.length == 0 ? \"{c}\" : String(params[0]);{l}"),
    }
}

/// `codegenResourceBundleMetadata(resourceBundleNames)`.
fn codegen_resource_bundle_metadata(resource_bundle_names: Option<&SortedSet>) -> String {
    let Some(names) = resource_bundle_names else { return String::new() };
    names.0.iter().map(|n| format!("[ResourceBundle(\"{n}\")]{LINE_SEP}")).collect()
}

fn attr<'a>(m: Option<&'a [(String, String)]>, k: &str) -> Option<&'a str> {
    m?.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
}

#[allow(clippy::too_many_arguments)]
fn codegen_imports(base: &str, root_attributes: Option<&[(String, String)]>, root_attribute_embed_vars: Option<&[(String, String)]>, fonts: &[TagRef], configuration: &Configuration, is_library_compile: bool, has_fonts: bool) -> String {
    let l = LINE_SEP;
    let mut sb = String::with_capacity(512);
    sb.push_str(&codegen_event_import(is_library_compile, has_fonts));
    for i in ["flash.display.LoaderInfo;", "flash.text.Font;", "flash.text.TextFormat;", "flash.text.engine.TextBlock;", "flash.text.engine.TextLine;", "flash.system.ApplicationDomain;", "flash.system.Security", "flash.utils.Dictionary;", "flash.utils.getDefinitionByName;", "flashx.textLayout.compose.ISWFContext;"] {
        sb.push_str(&format!("import {i}{l}"));
    }
    sb.push_str(&format!("import {INTERFACE_IFLEXMODULE_DOT};{l}"));
    sb.push_str(&format!("import {INTERFACE_IFLEXMODULEFACTORY_DOT};{l}"));
    sb.push_str(&codegen_import_embedded_font_registry(fonts));
    sb.push_str(&format!("import mx.core.RSLData;{l}"));
    sb.push_str(&format!("import mx.events.RSLEvent;{l}"));
    if configuration.css.compatibility_version_string().is_some() {
        sb.push_str(&format!("import {CLASS_FLEXVERSION_DOT};{l}"));
    }
    if !configuration.css.qualified_type_selectors {
        sb.push_str(&format!("import {CLASS_STYLEMANAGER_DOT};{l}"));
        sb.push_str(&format!("import {NAMESPACE_MX_INTERNAL_DOT};{l}"));
    }
    sb.push_str(&format!("import {base};{l}"));
    if let Some(c) = get_runtime_dpi_provider_class(root_attributes) {
        sb.push_str(&format!("import {c};{l}"));
    }
    if let Some(p) = get_preloader_class(root_attributes, configuration) {
        sb.push_str(&format!("import {p};{l}"));
    }
    if let Some(splash) = attr(root_attributes, "splashScreenImage") {
        if attr(root_attribute_embed_vars, "splashScreenImage").is_none() {
            sb.push_str(&format!("import {splash};{l}"));
        }
    }
    sb.push_str(l);
    sb
}

fn get_runtime_dpi_provider_class(root_attributes: Option<&[(String, String)]>) -> Option<String> {
    attr(root_attributes, "runtimeDPIProvider").map(str::to_string)
}

fn get_preloader_class(root_attributes: Option<&[(String, String)]>, configuration: &Configuration) -> Option<String> {
    if let Some(p) = attr(root_attributes, "preloader") {
        return Some(p.to_string());
    }
    if let Some(p) = &configuration.flex.preloader {
        let p = p.trim_matches(|c: char| c <= ' ');
        return if p.is_empty() { None } else { Some(p.to_string()) };
    }
    if configuration.css.compatibility_version() < VERSION_4_0 {
        Some("mx.preloaders.DownloadProgressBar".to_string())
    } else {
        Some("mx.preloaders.SparkDownloadProgressBar".to_string())
    }
}

fn codegen_event_import(is_library_compile: bool, has_fonts: bool) -> String {
    if !(is_library_compile && has_fonts) {
        return String::new();
    }
    format!("import flash.events.Event{LINE_SEP}")
}

fn codegen_embedded_applicaiton_attributes(root_attribute_embed_vars: Option<&[(String, String)]>) -> String {
    let Some(vars) = root_attribute_embed_vars.filter(|v| !v.is_empty()) else { return String::new() };
    let values: Vec<&str> = vars.iter().map(|(_, v)| v.as_str()).collect();
    format!("{}{LINE_SEP}", values.join(LINE_SEP))
}

/// `codegenInfo(...)`.
#[allow(clippy::too_many_arguments)]
fn codegen_info(
    top_level_window_class: Option<&str>,
    root_attributes: Option<&[(String, String)]>,
    root_attribute_embed_names: Option<&[(String, String)]>,
    cd_rsls: &[RslPathInfo],
    rsls: &[String],
    mixins: &[String],
    fonts: &[TagRef],
    frames: &[FrameInfo],
    locales: Option<&[String]>,
    resource_bundle_names: &[String],
    configuration: &Configuration,
) -> String {
    let mut t: SortedMap<String> = SortedMap::new();
    t.put("currentDomain", "ApplicationDomain.currentDomain".to_string());
    if let Some(c) = top_level_window_class {
        t.put("mainClassName", format!("\"{c}\""));
    }
    if let Some(attrs) = root_attributes {
        for (k, v) in attrs {
            match k.as_str() {
                "preloader" | "runtimeDPIProvider" | "implements" => {}
                "usePreloader" => {
                    t.put(k, v.clone());
                }
                "backgroundColor" => {
                    t.put(k, format!("\"0x{}\"", format!("{:x}", configuration.background_color as u32).to_uppercase()));
                }
                _ => {
                    let embed_name = attr(root_attribute_embed_names, k);
                    if let Some(e) = embed_name {
                        t.put(k, e.to_string());
                    } else if k == "splashScreenImage" {
                        t.put(k, v.clone());
                    } else {
                        t.put(k, format!("\"{v}\""));
                    }
                }
            }
        }
        if let Some(p) = get_preloader_class(root_attributes, configuration) {
            t.put("preloader", p);
        }
        if let Some(p) = get_runtime_dpi_provider_class(root_attributes) {
            t.put("runtimeDPIProvider", p);
        }
    }
    let has_cd_rsls = !cd_rsls.is_empty();
    let has_rsls = !rsls.is_empty();
    if has_cd_rsls || has_rsls {
        toolkit::log_plain(Level::Info, "Required RSLs:");
    }
    if has_cd_rsls {
        toolkit::log_plain(Level::Error, "Cross-domain RSLs (-runtime-shared-library-path) are not supported by this port.");
    }
    if has_rsls {
        t.put("rsls", codegen_rsl_list(rsls));
    }
    if !mixins.is_empty() {
        t.put("mixins", codegen_mixin_list(mixins));
    }
    if !fonts.is_empty() {
        t.put("fonts", codegen_font_list(fonts));
    }
    if !frames.is_empty() {
        t.put("frames", codegen_frame_class_list(frames));
    }
    if let Some(locales) = locales {
        t.put("compiledLocales", format!("[ {} ]", quoted_list(locales)));
    }
    if !resource_bundle_names.is_empty() {
        t.put("compiledResourceBundleNames", format!("[ {} ]", quoted_list(resource_bundle_names)));
    }
    let sep = format!(",{LINE_SEP}            ");
    let items: Vec<String> = t.0.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    format!("            {}{LINE_SEP}", items.join(&sep))
}

fn codegen_add_rsl_complete_listener(is_library_compile: bool, has_fonts: bool) -> String {
    if !(is_library_compile && has_fonts) {
        return String::new();
    }
    format!("        this.root.loaderInfo.addEventListener(Event.COMPLETE, RSLRootCompleteListener);{LINE_SEP}")
}

fn codegen_rsl_complete_listener(is_library_compile: bool, has_fonts: bool) -> String {
    if !(is_library_compile && has_fonts) {
        return String::new();
    }
    let l = LINE_SEP;
    [
        "    private function RSLRootCompleteListener(event:Event):void", l,
        "    {", l,
        "        EmbeddedFontRegistry.registerFonts(info()[\"fonts\"], this)", l,
        "        this.root.removeEventListener(Event.COMPLETE, RSLRootCompleteListener);", l,
        "    }", l,
    ]
    .concat()
}

fn codegen_module_factory_security_wrapper(is_library_compile: bool, has_fonts: bool) -> String {
    if is_library_compile && !has_fonts {
        return String::new();
    }
    if is_library_compile {
        return codegen_module_factory_security_wrapper_stubs();
    }
    let l = LINE_SEP;
    [
        "    /**", l, "     *  @private", l, "     */", l,
        "    private var _preloadedRSLs:Dictionary; // key: LoaderInfo, value: Vector.<RSLData>", l, l,
        "    /**", l, "     *  @private", l, "     */", l,
        "    private var _allowDomainParameters:Vector.<Array>;", l, l,
        "    /**", l, "     *  @private", l, "     */", l,
        "    private var _allowInsecureDomainParameters:Vector.<Array>;", l, l,
        "    /**", l, "     *  @private", l,
        "     *  The RSLs loaded by this system manager before the application", l,
        "     *  starts. RSLs loaded by the application are not included in this list.", l,
        "     */", l,
        "    override public function get preloadedRSLs():Dictionary", l,
        "    {", l,
        "        if (_preloadedRSLs == null)", l,
        "           _preloadedRSLs = new Dictionary(true);", l,
        "        return _preloadedRSLs;", l,
        "    }", l, l,
        "    /**", l, "     *  @private", l,
        "     *  Calls Security.allowDomain() for the SWF associated with this IFlexModuleFactory", l,
        "     *  plus all the SWFs assocatiated with RSLs preLoaded by this IFlexModuleFactory.", l,
        "     *", l,
        "     */", l,
        "    override public function allowDomain(... domains):void", l,
        "    {", l,
        "        Security.allowDomain.apply(null, domains);", l, l,
        "        for (var loaderInfo:Object in _preloadedRSLs)", l,
        "        {", l,
        "            if (loaderInfo.content && (\"allowDomainInRSL\" in loaderInfo.content))", l,
        "                loaderInfo.content[\"allowDomainInRSL\"].apply(null, domains);", l,
        "        }", l, l,
        "        if (!_allowDomainParameters)", l,
        "            _allowDomainParameters = new Vector.<Array>();", l,
        "        _allowDomainParameters.push(domains);", l, l,
        "        // Run our handler before the default handlers so the RSL is trusted before the", l,
        "        // default handlers run.", l,
        "        addEventListener(RSLEvent.RSL_ADD_PRELOADED, addPreloadedRSLHandler, false, 50);", l,
        "    }", l, l,
        "    /**", l, "     *  @private", l,
        "     *  Calls Security.allowInsecureDomain() for the SWF associated with this IFlexModuleFactory", l,
        "     *  plus all the SWFs assocatiated with RSLs preLoaded by this IFlexModuleFactory.", l,
        "     *", l,
        "     */", l,
        "    override public function allowInsecureDomain(... domains):void", l,
        "    {", l,
        "        Security.allowInsecureDomain.apply(null, domains);", l, l,
        "        for (var loaderInfo:Object in _preloadedRSLs)", l,
        "        {", l,
        "            if (loaderInfo.content && (\"allowInsecureDomainInRSL\" in loaderInfo.content))", l,
        "                loaderInfo.content[\"allowInsecureDomainInRSL\"].apply(null, domains);", l,
        "        }", l,
        "        if (!_allowInsecureDomainParameters)", l,
        "            _allowInsecureDomainParameters = new Vector.<Array>();", l,
        "        _allowInsecureDomainParameters.push(domains);", l, l,
        "        // Run our handler before the default handlers so the RSL is trusted before the", l,
        "        // default handlers run.", l,
        "        addEventListener(RSLEvent.RSL_ADD_PRELOADED, addPreloadedRSLHandler, false, 50);", l,
        "    }", l, l,
        "    /**", l, "     *  @private", l, "     */", l,
        "    private function addPreloadedRSLHandler(event:RSLEvent):void", l,
        "    {", l,
        "        var loaderInfo:LoaderInfo = event.loaderInfo;", l,
        "        if (!loaderInfo || !loaderInfo.content)", l,
        "            return;", l,
        "        var domains:Array", l,
        "        if (allowDomainsInNewRSLs && _allowDomainParameters)", l,
        "        {", l,
        "            for each (domains in _allowDomainParameters)", l,
        "            {", l,
        "                if (\"allowDomainInRSL\" in loaderInfo.content)", l,
        "                    loaderInfo.content[\"allowDomainInRSL\"].apply(null, domains);", l,
        "            }", l,
        "        }", l, l,
        "        if (allowInsecureDomainsInNewRSLs && _allowInsecureDomainParameters)", l,
        "        {", l,
        "            for each (domains in _allowInsecureDomainParameters)", l,
        "            {", l,
        "                if (\"allowInsecureDomainInRSL\" in loaderInfo.content)", l,
        "                    loaderInfo.content[\"allowInsecureDomainInRSL\"].apply(null, domains);", l,
        "            }", l,
        "        }", l,
        "    }", l, l,
    ]
    .concat()
}

fn codegen_module_factory_security_wrapper_stubs() -> String {
    let l = LINE_SEP;
    let stub = |sig: &str, body: Option<&str>, extra_star: bool| -> String {
        let mut s = format!("    /**{l}     *  @private{l}     *  Stub for RSL{l}");
        if extra_star {
            s.push_str(&format!("     *{l}"));
        }
        s.push_str(&format!("     */{l}    {sig}{l}    {{{l}"));
        if let Some(b) = body {
            s.push_str(&format!("        {b}{l}"));
        }
        s.push_str(&format!("    }}{l}{l}"));
        s
    };
    [
        stub("public function get allowDomainsInNewRSLs():Boolean", Some("return false;"), false),
        stub("public function set allowDomainsInNewRSLs(value:Boolean):void", None, false),
        stub("public function get allowInsecureDomainsInNewRSLs():Boolean", Some("return false;"), false),
        stub("public function set allowInsecureDomainsInNewRSLs(value:Boolean):void", None, false),
        stub("public function get preloadedRSLs():Dictionary", Some("return null;"), false),
        stub("public function allowDomain(... domains):void", None, false),
        stub("public function allowInsecureDomain(... domains):void", None, true),
    ]
    .concat()
}

/// `codegenRSLSecurityWrapper(isLibraryCompile, lineSep)`.
pub fn codegen_rsl_security_wrapper(is_library_compile: bool) -> String {
    if !is_library_compile {
        return String::new();
    }
    let l = LINE_SEP;
    [
        "   /*", l,
        "    *  Calls Security.allowDomain() for the SWF associated with this RSL", l,
        "    *  @param a list of domains to trust. This parameter is passed to Security.allowDomain().", l,
        "    */", l,
        "   public function allowDomainInRSL(... domains):void", l,
        "   {", l,
        "       Security.allowDomain.apply(null, domains);", l,
        "   }", l, l,
        "   /*", l,
        "    *  Calls Security.allowInsecureDomain() for the SWF associated with this RSL", l,
        "    *  @param a list of domains to trust. This parameter is passed to Security.allowInsecureDomain().", l,
        "    */", l,
        "   public function allowInsecureDomainInRSL(... domains):void", l,
        "   {", l,
        "       Security.allowInsecureDomain.apply(null, domains);", l,
        "   }", l,
    ]
    .concat()
}

fn codegen_get_register_implementation_stubs(is_library_compile: bool) -> String {
    if !is_library_compile {
        return String::new();
    }
    let l = LINE_SEP;
    [
        "    /**", l, "     *  @private", l, "     *  Stub for RSL", l, "     */", l,
        "    public function getImplementation(interfaceName:String):Object", l,
        "    {", l,
        "        return null;", l,
        "    }", l, l,
        "    /**", l, "     *  @private", l, "     *  Stub for RSL", l, "     */", l,
        "    public function registerImplementation(interfaceName:String,", l,
        "                                           impl:Object):void", l,
        "    {", l,
        "    }", l, l,
    ]
    .concat()
}

fn codegen_add_preloaded_rsl_stub(is_library_compile: bool) -> String {
    if !is_library_compile {
        return String::new();
    }
    let l = LINE_SEP;
    [
        "    /**", l, "     *  @private", l, "     *  Stub for RSL", l, "     */", l,
        "    public function addPreloadedRSL(loaderInfo:LoaderInfo, rsl:Vector.<RSLData>):void", l,
        "    {", l,
        "    }", l, l,
    ]
    .concat()
}

/// `processCompiledResourceBundleInfoClass(units, configuration, sources, mixins, fonts, swcContext)`.
fn process_compiled_resource_bundle_info_class(state: &State, configuration: &mut Configuration, sources: &mut Vec<SourceRef>) {
    if configuration.css.compatibility_version() < VERSION_3_0 {
        return;
    }
    let locales = configuration.flex.locales.clone();
    if locales.is_empty() || state.resource_bundle_names.0.is_empty() {
        return;
    }
    let class_name = i18n_utils::COMPILED_RESOURCE_BUNDLE_INFO;
    let code = i18n_utils::codegen_compiled_resource_bundle_info(&locales, &state.resource_bundle_names.0);
    let generated_file_name = format!("{class_name}-generated.as");
    if let Some(dir) = &configuration.generated_directory {
        save_generated(&generated_file_name, &code, dir);
    }
    let file = VirtualFile::InMemory { bytes: Rc::new(code.into_bytes()), name: generated_file_name, mime_type: mime_mappings::AS, last_modified: now() };
    sources.push(Source::new(file, None, "", class_name, Owner::None, false, false, false));
    configuration.includes.push(class_name.to_string());
}

#[cfg(test)]
mod tests {
    /// `LUMEN_PRELINK_APP=<Main.as>;<generated dir>;<out.swf>[;<extra flex-config file>]` compiles an application
    /// as the SDK's mxmlc does with its default `flex-config.xml`, writing the SWF and `<out.swf>.log`.
    #[test]
    fn prelink_app_compile() {
        let Ok(spec) = std::env::var("LUMEN_PRELINK_APP") else { return };
        let parts: Vec<&str> = spec.split(';').collect();
        let main = std::path::PathBuf::from(parts[0]);
        let fw = std::path::PathBuf::from(std::env::var("LUMEN_FLEXLIB").unwrap_or_else(|_| r"S:\Desktop\Projects\Rust-FFDec\flex-sdk-bin\sdk\frameworks".into()));
        let mut opts = crate::asc::flex2::compiler::file_configurator::mxmlc_defaults(&fw).unwrap();
        if let Some(extra) = parts.get(3).filter(|p| !p.is_empty()) {
            crate::asc::flex2::compiler::file_configurator::load(&mut opts, std::path::Path::new(extra), &fw).unwrap();
        }
        opts.main = main.clone();
        opts.source_path.insert(0, main.parent().unwrap().to_path_buf());
        opts.compile_date = Some(0);
        opts.compress = false;
        if !parts[1].is_empty() {
            opts.generated_directory = Some(std::path::PathBuf::from(parts[1]));
        }
        let mut log = String::new();
        match crate::asc::flex2::compile_application(&opts) {
            Ok(app) => {
                for d in &app.diagnostics {
                    log.push_str(&format!("{}\n\n", d.text));
                }
                std::fs::write(parts[2], &app.swf).unwrap();
            }
            Err(d) => {
                for d in &d {
                    log.push_str(&format!("{}\n\n", d.text));
                }
            }
        }
        std::fs::write(format!("{}.log", parts[2]), log).unwrap();
    }
}
