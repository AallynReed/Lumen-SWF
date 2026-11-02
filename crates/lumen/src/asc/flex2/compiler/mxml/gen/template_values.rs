//! The rep objects as `ClassDef.vm`/`ClassDefLib.vm` see them (`MxmlDocument`, `Model`, the initializers,
//! `BindingExpression`, `CodeFragmentList`, `StylesContainer`): Velocity `JavaObject`s with the Java public methods the
//! templates call, evaluated when called.

use std::borrow::Cow;
use std::rc::Rc;

use super::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::css::styles_container::{StyleDefList, StylesContainer};
use crate::asc::flex2::compiler::css::template_context;
use crate::asc::flex2::compiler::mxml::rep::at_embed::AtEmbedView;
use crate::asc::flex2::compiler::mxml::rep::binding_expression::BindingExpressionRef;
use crate::asc::flex2::compiler::mxml::rep::init::initializer::InitializerRef;
use crate::asc::flex2::compiler::mxml::rep::model::ModelRef;
use crate::asc::flex2::compiler::mxml::rep::mxml_document::{MxmlDocument, MxmlDocumentRef};
use crate::asc::flex2::velocity::{JType, JavaException, JavaObject, Method, Value};

const Z: JType = JType::Boolean;
const I: JType = JType::Int;
const S: JType = JType::String;
const fn c(name: &'static str) -> JType {
    JType::Class(Cow::Borrowed(name))
}
const ITER: JType = c("java.util.Iterator");
const LIST: JType = c("java.util.List");
const SET: JType = c("java.util.Set");
const COLL: JType = c("java.util.Collection");
const CFL: JType = c("flex2.compiler.mxml.gen.CodeFragmentList");
const MODEL: JType = c("flex2.compiler.mxml.rep.Model");
const BE: JType = c("flex2.compiler.mxml.rep.BindingExpression");

fn s(v: impl AsRef<str>) -> Value {
    Value::str(v.as_ref())
}

fn opt(v: Option<String>) -> Value {
    v.map_or(Value::Null, |v| Value::str(&v))
}

fn identity(class: &str, p: *const u8) -> String {
    format!("{class}@{:x}", p as usize)
}

/// A `CodeFragmentList` (an `ArrayList<Script>`).
pub fn code_fragment_list(list: &CodeFragmentList) -> Value {
    Value::list_of("flex2.compiler.mxml.gen.CodeFragmentList", list.iter().map(|s| s.value()).collect())
}

pub fn initializer(i: &InitializerRef) -> Value {
    Value::Object(Rc::new(InitializerView(i.clone())))
}

fn initializers(v: Vec<InitializerRef>) -> Value {
    Value::iterator(v.iter().map(initializer).collect())
}

pub fn model(m: &ModelRef) -> Value {
    Value::Object(Rc::new(ModelView(m.clone())))
}

pub fn binding_expression(b: &BindingExpressionRef) -> Value {
    Value::Object(Rc::new(BindingExpressionView(b.clone())))
}

pub fn document(d: &MxmlDocumentRef) -> Value {
    Value::Object(Rc::new(DocumentView(d.clone())))
}

pub fn styles_container(c: &Rc<StylesContainer>) -> Value {
    Value::Object(Rc::new(StylesContainerView(c.clone())))
}

struct InitializerView(InitializerRef);

static INITIALIZER_METHODS: &[Method] = &[
    Method::new("getLineRef", &[], I),
    Method::new("isBinding", &[], Z),
    Method::new("getValueExpr", &[], S),
    Method::new("getAssignExpr", &[S], S),
    Method::new("hasDefinition", &[], Z),
    Method::new("isStateSpecific", &[], Z),
    Method::new("getDefinitionsIterator", &[], ITER),
    Method::new("getName", &[], S),
];

impl JavaObject for InitializerView {
    fn class_name(&self) -> &str {
        match &*self.0 {
            crate::asc::flex2::compiler::mxml::rep::init::initializer::Initializer::Event(_) => "flex2.compiler.mxml.rep.init.EventInitializer",
            crate::asc::flex2::compiler::mxml::rep::init::initializer::Initializer::Value(v) => {
                use crate::asc::flex2::compiler::mxml::rep::init::value_initializer::ValueInitializerKind as K;
                match &v.kind {
                    K::StaticProperty { .. } => "flex2.compiler.mxml.rep.init.StaticPropertyInitializer",
                    K::DynamicProperty { .. } => "flex2.compiler.mxml.rep.init.DynamicPropertyInitializer",
                    K::Style { .. } => "flex2.compiler.mxml.rep.init.StyleInitializer",
                    K::Effect { .. } => "flex2.compiler.mxml.rep.init.EffectInitializer",
                    K::ArrayElement { .. } => "flex2.compiler.mxml.rep.init.ArrayElementInitializer",
                    K::VisualChild => "flex2.compiler.mxml.rep.init.VisualChildInitializer",
                    K::InitializedPropertyDeclaration { .. } => "flex2.compiler.mxml.rep.decl.InitializedPropertyDeclaration",
                }
            }
        }
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == self.class_name() || class_name == "flex2.compiler.mxml.rep.init.Initializer"
    }

    fn methods(&self) -> &[Method] {
        INITIALIZER_METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let i = &self.0;
        Ok(match &*method.name {
            "getLineRef" => Value::Integer(i.get_line_ref()),
            "isBinding" => Value::Boolean(i.is_binding()),
            "getValueExpr" => s(i.get_value_expr()),
            "getAssignExpr" => s(i.get_assign_expr(&args[0].to_java_string())),
            "hasDefinition" => Value::Boolean(i.has_definition()),
            "isStateSpecific" => Value::Boolean(i.is_state_specific()),
            "getDefinitionsIterator" => Value::iterator(i.get_definitions_iterator().iter().map(code_fragment_list).collect()),
            "getName" => s(i.get_name()),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        identity(self.class_name(), Rc::as_ptr(&self.0) as *const u8)
    }
}

struct ModelView(ModelRef);

static MODEL_METHODS: &[Method] = &[
    Method::new("getXmlLineNumber", &[], I),
    Method::new("getId", &[], S),
    Method::new("getIdIsAutogenerated", &[], Z),
    Method::new("getDefinitionName", &[], S),
    Method::new("getStyleAndEffectInitializerIterator", &[], ITER),
    Method::new("getEffectNames", &[], S),
    Method::new("getEffectEventNames", &[], S),
    Method::new("getEventInitializerIterator", &[], ITER),
    Method::new("getPropertyInitializerIterator", &[], ITER),
    Method::new("getPropertyInitializerIterator", &[Z], ITER),
    Method::new("getStyleInitializerIterator", &[], ITER),
    Method::new("getEffectInitializerIterator", &[], ITER),
    Method::new("isDeclared", &[], Z),
    Method::new("getParent", &[], MODEL),
];

impl JavaObject for ModelView {
    fn class_name(&self) -> &str {
        match self.0.kind_class_simple_name() {
            "Model" => "flex2.compiler.mxml.rep.Model",
            "AnonymousObjectGraph" => "flex2.compiler.mxml.rep.AnonymousObjectGraph",
            "DesignLayer" => "flex2.compiler.mxml.rep.DesignLayer",
            "Reparent" => "flex2.compiler.mxml.rep.Reparent",
            "Vector" => "flex2.compiler.mxml.rep.Vector",
            "Array" => "flex2.compiler.mxml.rep.Array",
            "Primitive" => "flex2.compiler.mxml.rep.Primitive",
            "XML" => "flex2.compiler.mxml.rep.XML",
            "XMLList" => "flex2.compiler.mxml.rep.XMLList",
            _ => "flex2.compiler.mxml.rep.MovieClip",
        }
    }

    fn instance_of(&self, class_name: &str) -> bool {
        class_name == self.class_name() || class_name == "flex2.compiler.mxml.rep.Model" || (class_name == "flex2.compiler.mxml.rep.Array" && self.0.is_array())
    }

    fn methods(&self) -> &[Method] {
        MODEL_METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let m = &self.0;
        Ok(match &*method.name {
            "getXmlLineNumber" => Value::Integer(m.get_xml_line_number()),
            "getId" => opt(m.get_id()),
            "getIdIsAutogenerated" => Value::Boolean(m.get_id_is_autogenerated()),
            "getDefinitionName" => opt(m.get_definition_name()),
            "getStyleAndEffectInitializerIterator" => initializers(m.get_style_and_effect_initializer_iterator()),
            "getEffectNames" => s(m.get_effect_names()),
            "getEffectEventNames" => s(m.get_effect_event_names()),
            "getEventInitializerIterator" => initializers(m.get_event_initializer_iterator()),
            "getPropertyInitializerIterator" => match args.first() {
                Some(Value::Boolean(b)) => initializers(m.get_property_initializer_iterator_bindings(*b)),
                _ => initializers(m.get_property_initializer_iterator()),
            },
            "getStyleInitializerIterator" => initializers(m.get_style_initializer_iterator()),
            "getEffectInitializerIterator" => initializers(m.get_effect_initializer_iterator()),
            "isDeclared" => Value::Boolean(m.is_declared()),
            "getParent" => m.get_parent().map_or(Value::Null, |p| model(&p)),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        identity(self.class_name(), Rc::as_ptr(&self.0) as *const u8)
    }

    fn java_equals(&self, other: &Value) -> bool {
        match other {
            Value::Object(o) => o.to_java_string() == self.to_java_string(),
            _ => false,
        }
    }
}

struct BindingExpressionView(BindingExpressionRef);

static BINDING_EXPRESSION_METHODS: &[Method] = &[
    Method::new("isRepeatable", &[], Z),
    Method::new("getId", &[], I),
    Method::new("getDestinationTypeName", &[], S),
    Method::new("getRepeatableSourceExpression", &[], S),
    Method::new("getXmlLineNumber", &[], I),
    Method::new("getSourceExpression", &[], S),
    Method::new("isStyle", &[], Z),
    Method::new("getDestinationPathRoot", &[Z], S),
    Method::new("getDestinationLValue", &[], S),
    Method::new("isDestinationObjectProxy", &[], Z),
    Method::new("getDestinationPath", &[Z], S),
    Method::new("isSourcePublicProperty", &[], Z),
    Method::new("getTwoWayCounterpart", &[], BE),
    Method::new("getNamespaceDeclarations", &[], S),
    Method::new("isSimpleChain", &[], Z),
    Method::new("isDestinationNonPublicProperty", &[], Z),
    Method::new("getDestinationStyle", &[], S),
    Method::new("getSourceAsProperty", &[], S),
    Method::new("isTwoWayPrimary", &[], Z),
    Method::new("getDestination", &[], MODEL),
    Method::new("getDestinationProperty", &[], S),
    Method::new("isArrayAccess", &[], Z),
    Method::new("isFromBindingNode", &[], Z),
    Method::new("isDestinationXMLAttribute", &[], Z),
    Method::new("isDestinationXMLNode", &[], Z),
    Method::new("isDestinationE4X", &[], Z),
];

impl JavaObject for BindingExpressionView {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.BindingExpression"
    }

    fn methods(&self) -> &[Method] {
        BINDING_EXPRESSION_METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let b = &self.0;
        let flag = || matches!(args.first(), Some(Value::Boolean(true)));
        Ok(match &*method.name {
            "isRepeatable" => Value::Boolean(b.is_repeatable()),
            "getId" => Value::Integer(b.get_id()),
            "getDestinationTypeName" => s(b.get_destination_type_name()),
            "getRepeatableSourceExpression" => s(b.get_repeatable_source_expression()),
            "getXmlLineNumber" => Value::Integer(b.get_xml_line_number()),
            "getSourceExpression" => s(b.get_source_expression()),
            "isStyle" => Value::Boolean(b.is_style()),
            "getDestinationPathRoot" => s(b.get_destination_path_root(flag())),
            "getDestinationLValue" => opt(b.get_destination_lvalue()),
            "isDestinationObjectProxy" => Value::Boolean(b.is_destination_object_proxy()),
            "getDestinationPath" => s(b.get_destination_path(flag())),
            "isSourcePublicProperty" => Value::Boolean(b.is_source_public_property()),
            "getTwoWayCounterpart" => b.get_two_way_counterpart().map_or(Value::Null, |t| binding_expression(&t)),
            "getNamespaceDeclarations" => s(b.get_namespace_declarations()),
            "isSimpleChain" => Value::Boolean(b.is_simple_chain()),
            "isDestinationNonPublicProperty" => Value::Boolean(b.is_destination_non_public_property()),
            "getDestinationStyle" => opt(b.get_destination_style()),
            "getSourceAsProperty" => opt(b.get_source_as_property()),
            "isTwoWayPrimary" => Value::Boolean(b.is_two_way_primary()),
            "getDestination" => b.get_destination().map_or(Value::Null, |m| model(&m)),
            "getDestinationProperty" => opt(b.get_destination_property()),
            "isArrayAccess" => Value::Boolean(b.is_array_access()),
            "isFromBindingNode" => Value::Boolean(b.is_from_binding_node()),
            "isDestinationXMLAttribute" => Value::Boolean(b.is_destination_xml_attribute()),
            "isDestinationXMLNode" => Value::Boolean(b.is_destination_xml_node()),
            "isDestinationE4X" => Value::Boolean(b.is_destination_e4x()),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        identity(self.class_name(), Rc::as_ptr(&self.0) as *const u8)
    }

    fn java_equals(&self, other: &Value) -> bool {
        match other {
            Value::Object(o) if o.class_name() == self.class_name() => o.invoke(&Method::new("getId", &[], I), &[]).ok().is_some_and(|v| v.java_equals(&Value::Integer(self.0.get_id()))),
            _ => false,
        }
    }

    fn java_hash_code(&self) -> i32 {
        Rc::as_ptr(&self.0) as usize as i32
    }
}

struct StylesContainerView(Rc<StylesContainer>);

static STYLES_CONTAINER_METHODS: &[Method] = &[Method::new("getStyleDefs", &[], COLL), Method::new("isAdvanced", &[], Z)];

impl JavaObject for StylesContainerView {
    fn class_name(&self) -> &str {
        "flex2.compiler.css.StylesContainer"
    }

    fn methods(&self) -> &[Method] {
        STYLES_CONTAINER_METHODS
    }

    fn invoke(&self, method: &Method, _args: &[Value]) -> Result<Value, JavaException> {
        Ok(match &*method.name {
            "getStyleDefs" => {
                let list = template_context::style_def_list(&StyleDefList { style_defs: self.0.get_style_defs() });
                let defs = match &list {
                    Value::Object(o) => o.invoke(&Method::new("getStyleDefs", &[], LIST), &[])?,
                    _ => Value::Null,
                };
                match defs {
                    Value::List(l) => Value::list_of("java.util.LinkedHashMap$LinkedValues", l.items.borrow().clone()),
                    v => v,
                }
            }
            "isAdvanced" => Value::Boolean(self.0.is_advanced()),
            _ => Value::Null,
        })
    }
}

struct DocumentView(MxmlDocumentRef);

static DOCUMENT_METHODS: &[Method] = &[
    Method::new("getPackageName", &[], S),
    Method::new("getClassName", &[], S),
    Method::new("getConvertedClassName", &[], S),
    Method::new("getSourcePath", &[], S),
    Method::new("getImports", &[], SET),
    Method::new("getAtResources", &[], COLL),
    Method::new("getMetadata", &[], LIST),
    Method::new("getComment", &[], S),
    Method::new("getRoot", &[], MODEL),
    Method::new("getSuperClassName", &[], S),
    Method::new("getHasInterfaces", &[], Z),
    Method::new("getInterfaceList", &[], S),
    Method::new("getDeclarationIterator", &[], ITER),
    Method::new("getTypeRefs", &[], COLL),
    Method::new("getIsIUIComponent", &[], Z),
    Method::new("getDescribeVisualChildren", &[], Z),
    Method::new("getIsContainer", &[], Z),
    Method::new("getDescriptorDeclaration", &[S], CFL),
    Method::new("getIsVisualElementContainer", &[], Z),
    Method::new("getStatesDeclaration", &[], CFL),
    Method::new("getIsIFlexModule", &[], Z),
    Method::new("getHasStagePropertyInitializers", &[], Z),
    Method::new("getScripts", &[], LIST),
    Method::new("getAtEmbeds", &[], SET),
    Method::new("getBindingExpressions", &[], LIST),
    Method::new("getBindingManagementVars", &[], LIST),
    Method::new("superHasPublicProperty", &[S], Z),
    Method::new("getStylesContainer", &[], c("flex2.compiler.css.StylesContainer")),
    Method::new("getIsFlexApplication", &[], Z),
    Method::new("getDesignLayerPropertyInitializerIterator", &[], ITER),
    Method::new("getNonStagePropertyInitializerIterator", &[], ITER),
    Method::new("getStagePropertyInitializerIterator", &[], ITER),
    Method::new("getProceduralVisualChildInitializerIterator", &[], ITER),
    Method::new("getWatcherSetupUtilClassName", &[], S),
    Method::new("hasBindingTags", &[], Z),
    Method::new("getAllBindingNamespaceDeclarations", &[], S),
    Method::new("getDefinitionIterator", &[], ITER),
    Method::new("getIsMain", &[], Z),
    Method::new("getIsInlineComponent", &[], Z),
    Method::new("getIsSimpleStyleComponent", &[], Z),
    Method::new("getVersion", &[], I),
    Method::new("getLanguageNamespace", &[], S),
    Method::new("getPreloader", &[], S),
    Method::new("getUsePreloader", &[], Z),
    Method::new("getTopLevelInitializerIterator", &[], ITER),
    Method::new("getPropertyInitializerIterator", &[], ITER),
    Method::new("getStatefulEventIterator", &[], ITER),
];

impl JavaObject for DocumentView {
    fn class_name(&self) -> &str {
        "flex2.compiler.mxml.rep.MxmlDocument"
    }

    fn methods(&self) -> &[Method] {
        DOCUMENT_METHODS
    }

    fn invoke(&self, method: &Method, args: &[Value]) -> Result<Value, JavaException> {
        let d: &MxmlDocument = &self.0;
        Ok(match &*method.name {
            "getPackageName" => s(d.get_package_name()),
            "getClassName" => s(d.get_class_name()),
            "getConvertedClassName" => s(d.get_converted_class_name()),
            "getSourcePath" => s(d.get_source_path()),
            "getImports" => Value::list_of("java.util.TreeSet", d.get_imports().iter().map(|n| n.value()).collect()),
            "getAtResources" => Value::list_of("java.util.TreeMap$Values", d.get_at_resources().into_iter().map(|r| Value::Object(r as Rc<dyn JavaObject>)).collect()),
            "getMetadata" => Value::list(d.get_metadata().iter().map(|s| s.value()).collect()),
            "getComment" => opt(d.get_comment()),
            "getRoot" => model(&d.get_root()),
            "getSuperClassName" => s(d.get_super_class_name()),
            "getHasInterfaces" => Value::Boolean(d.get_has_interfaces()),
            "getInterfaceList" => s(d.get_interface_list()),
            "getDeclarationIterator" => Value::iterator(d.get_declaration_iterator().iter().map(|p| p.value()).collect()),
            "getTypeRefs" => Value::list_of("java.util.TreeSet", d.get_type_refs().iter().map(s).collect()),
            "getIsIUIComponent" => Value::Boolean(d.get_is_iui_component()),
            "getDescribeVisualChildren" => Value::Boolean(d.get_describe_visual_children()),
            "getIsContainer" => Value::Boolean(d.get_is_container()),
            "getDescriptorDeclaration" => code_fragment_list(&d.get_descriptor_declaration(&args[0].to_java_string())),
            "getIsVisualElementContainer" => Value::Boolean(d.get_is_visual_element_container()),
            "getStatesDeclaration" => code_fragment_list(&d.get_states_declaration()),
            "getIsIFlexModule" => Value::Boolean(d.get_is_iflex_module()),
            "getHasStagePropertyInitializers" => Value::Boolean(d.get_has_stage_property_initializers()),
            "getScripts" => Value::list(d.get_scripts().iter().map(|s| s.value()).collect()),
            "getAtEmbeds" => Value::list_of("java.util.HashSet", d.get_at_embeds().into_iter().map(|e| Value::Object(Rc::new(AtEmbedView(e)))).collect()),
            "getBindingExpressions" => Value::list(d.get_binding_expressions().iter().map(binding_expression).collect()),
            "getBindingManagementVars" => Value::list(MxmlDocument::get_binding_management_vars().iter().map(|v| v.value()).collect()),
            "superHasPublicProperty" => Value::Boolean(d.super_has_public_property(&args[0].to_java_string())),
            "getStylesContainer" => styles_container(&d.get_styles_container()),
            "getIsFlexApplication" => Value::Boolean(d.get_is_flex_application()),
            "getDesignLayerPropertyInitializerIterator" => initializers(d.get_design_layer_property_initializer_iterator()),
            "getNonStagePropertyInitializerIterator" => initializers(d.get_non_stage_property_initializer_iterator()),
            "getStagePropertyInitializerIterator" => initializers(d.get_stage_property_initializer_iterator()),
            "getProceduralVisualChildInitializerIterator" => initializers(d.get_procedural_visual_child_initializer_iterator()),
            "getWatcherSetupUtilClassName" => s(d.get_watcher_setup_util_class_name()),
            "hasBindingTags" => Value::Boolean(d.has_binding_tags()),
            "getAllBindingNamespaceDeclarations" => s(d.get_all_binding_namespace_declarations()),
            "getDefinitionIterator" => Value::iterator(d.get_definition_iterator().iter().map(code_fragment_list).collect()),
            "getIsMain" => Value::Boolean(d.get_is_main()),
            "getIsInlineComponent" => Value::Boolean(d.get_is_inline_component()),
            "getIsSimpleStyleComponent" => Value::Boolean(d.get_is_simple_style_component()),
            "getVersion" => Value::Integer(d.get_version()),
            "getLanguageNamespace" => opt(d.get_language_namespace()),
            "getPreloader" => s(d.get_preloader()),
            "getUsePreloader" => Value::Boolean(d.get_use_preloader()),
            "getTopLevelInitializerIterator" => initializers(d.get_top_level_initializer_iterator()),
            "getPropertyInitializerIterator" => initializers(d.get_property_initializer_iterator()),
            "getStatefulEventIterator" => initializers(d.get_stateful_event_iterator()),
            _ => Value::Null,
        })
    }

    fn to_java_string(&self) -> String {
        identity(self.class_name(), Rc::as_ptr(&self.0) as *const u8)
    }
}
