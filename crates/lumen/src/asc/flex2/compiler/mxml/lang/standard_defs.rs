//! Port of `flex2.compiler.mxml.lang.StandardDefs` (only `StandardDefs2006` exists, so its package getters are
//! folded into the field values).

use std::rc::Rc;

use crate::asc::flex2::compiler::mxml::dom::node::NodeRef;
use crate::asc::flex2::compiler::mxml::reflect::type_table::Type;
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::util::jhash::{JHashMap, JHashSet};

pub const SWC_AIRGLOBAL: &str = "airglobal.swc";
pub const SWC_AVMPLUS: &str = "avmplus.swc";
pub const SWC_PLAYERGLOBAL: &str = "playerglobal.swc";
pub const PACKAGE___AS3___VEC: &str = "__AS3__.vec";
pub const PACKAGE_FLASH_ACCESSIBILITY: &str = "flash.accessibility";
pub const PACKAGE_FLASH_DATA: &str = "flash.data";
pub const PACKAGE_FLASH_DEBUGGER: &str = "flash.debugger";
pub const PACKAGE_FLASH_DESKTOP: &str = "flash.desktop";
pub const PACKAGE_FLASH_DISPLAY: &str = "flash.display";
pub const PACKAGE_FLASH_ERRORS: &str = "flash.errors";
pub const PACKAGE_FLASH_EVENTS: &str = "flash.events";
pub const PACKAGE_FLASH_EXTERNAL: &str = "flash.external";
pub const PACKAGE_FLASH_FILESYSTEM: &str = "flash.filesystem";
pub const PACKAGE_FLASH_FILTERS: &str = "flash.filters";
pub const PACKAGE_FLASH_GEOM: &str = "flash.geom";
pub const PACKAGE_FLASH_HTML: &str = "flash.html";
pub const PACKAGE_FLASH_HTML_SCRIPT: &str = "flash.html.script";
pub const PACKAGE_FLASH_MEDIA: &str = "flash.media";
pub const PACKAGE_FLASH_NET: &str = "flash.net";
pub const PACKAGE_FLASH_PRINTING: &str = "flash.printing";
pub const PACKAGE_FLASH_PROFILER: &str = "flash.profiler";
pub const PACKAGE_FLASH_SYSTEM: &str = "flash.system";
pub const PACKAGE_FLASH_TEXT: &str = "flash.text";
pub const PACKAGE_FLASH_UI: &str = "flash.ui";
pub const PACKAGE_FLASH_UTILS: &str = "flash.utils";
pub const PACKAGE_FLASH_XML: &str = "flash.xml";
pub const PACKAGE_FLASH_TEXTLAYOUT_COMPOSE: &str = "flashx.textLayout.compose";
pub const PACKAGE_MX_BINDING: &str = "mx.binding";
pub const PACKAGE_MX_CONTAINERS: &str = "mx.containers";
pub const PACKAGE_MX_CONTROLS: &str = "mx.controls";
pub const PACKAGE_MX_CORE: &str = "mx.core";
pub const PACKAGE_MX_DATA: &str = "mx.data";
pub const PACKAGE_MX_DATA_UTILS: &str = "mx.data.utils";
pub const PACKAGE_MX_EFFECTS: &str = "mx.effects";
pub const PACKAGE_MX_EVENTS: &str = "mx.events";
pub const PACKAGE_MX_FILTERS: &str = "mx.filters";
pub const PACKAGE_MX_MANAGERS: &str = "mx.managers";
pub const PACKAGE_MX_MESSAGING_CONFIG: &str = "mx.messaging.config";
pub const PACKAGE_MX_MODULES: &str = "mx.modules";
pub const PACKAGE_MX_PRELOADERS: &str = "mx.preloaders";
pub const PACKAGE_MX_RESOURCES: &str = "mx.resources";
pub const PACKAGE_MX_RPC: &str = "mx.rpc";
pub const PACKAGE_MX_RPC_XML: &str = "mx.rpc.xml";
pub const PACKAGE_MX_STATES: &str = "mx.states";
pub const PACKAGE_MX_STYLES: &str = "mx.styles";
pub const PACKAGE_MX_UTILS: &str = "mx.utils";
pub const PACKAGE_SPARK_COMPONENTS: &str = "spark.components";
pub const PACKAGE_SPARK_CORE: &str = "spark.core";
pub const PACKAGE_SPARK_PRIMITIVES: &str = "spark.primitives";
pub const PACKAGE_TEXT_ELEMENTS: &str = "flashx.textLayout.elements";
pub const PACKAGE_TEXT_FORMATS: &str = "flashx.textLayout.formats";
pub const NAMESPACE_MX_INTERNAL_LOCALNAME: &str = "mx_internal";
pub const NAMESPACE_MX_INTERNAL_URI: &str = "http://www.adobe.com/2006/flex/mx/internal";
pub const INTERFACE_FLASH_TEXTLAYOUT_ISWFCONTEXT: &str = "flashx.textLayout.compose:ISWFContext";
pub const INTERFACE_IMODULE_NO_PACKAGE: &str = "IModule";
pub const CLASS_APPLICATION: &str = "mx.core:Application";
pub const CLASS_ARRAY: &str = "Array";
pub const CLASS_VECTOR: &str = "__AS3__.vec.Vector";
pub const CLASS_VECTOR_SHORTNAME: &str = "Vector";
pub const CLASS_XML: &str = "XML";
pub const CLASS_XMLLIST: &str = "XMLList";
pub const CLASS_XMLNODE: &str = "flash.xml:XMLNode";
pub const CLASS_SPARK_APPLICATION: &str = "spark.components:Application";
pub const CLASS_SPARK_SPRITEVISUALELEMENT: &str = "spark.core:SpriteVisualElement";
pub const CLASS_TEXT_DIV: &str = "flashx.textLayout.elements:DivElement";
pub const CLASS_TEXT_IMG: &str = "flashx.textLayout.elements:InlineGraphicElement";
pub const CLASS_TEXT_LINK: &str = "flashx.textLayout.elements:LinkElement";
pub const CLASS_TEXT_LAYOUT_FORMAT: &str = "flashx.textLayout.formats:TextLayoutFormat";
pub const CLASS_TEXT_RICHTEXT: &str = "spark.components:RichText";
pub const CLASS_TEXT_PARAGRAPH: &str = "flashx.textLayout.elements:ParagraphElement";
pub const CLASS_TEXT_SPAN: &str = "flashx.textLayout.elements:SpanElement";
pub const CLASS_TEXT_TAB: &str = "flashx.textLayout.elements:TabElement";
pub const CLASS_TEXT_TCY: &str = "flashx.textLayout.elements:TCYElement";
pub const PROP_CURRENTSTATE: &str = "currentState";
pub const PROP_EXCLUDE_STATES: &str = "excludeFrom";
pub const PROP_FORMAT: &str = "format";
pub const PROP_CLASSFACTORY_GENERATOR: &str = "generator";
pub const PROP_ID: &str = "id";
pub const PROP_FIXED: &str = "fixed";
pub const PROP_TYPE: &str = "type";
pub const PROP_INCLUDE_STATES: &str = "includeIn";
pub const PROP_ITEM_CREATION_POLICY: &str = "itemCreationPolicy";
pub const PROP_ITEM_DESTRUCTION_POLICY: &str = "itemDestructionPolicy";
pub const PROP_STATE_NAME: &str = "name";
pub const PROP_STATE_GROUPS: &str = "stateGroups";
pub const PROP_SOURCE: &str = "source";
pub const PROP_CLASSFACTORY_PROPERTIES: &str = "properties";
pub const PROP_UICOMPONENT_STATES: &str = "states";
pub const PROP_CONTAINER_CHILDREPEATERS: &str = "childRepeaters";
pub const MD_ACCESSIBILITYCLASS: &str = "AccessibilityClass";
pub const MD_ARRAYELEMENTTYPE: &str = "ArrayElementType";
pub const MD_BINDABLE: &str = "Bindable";
pub const MD_CHANGEEVENT: &str = "ChangeEvent";
pub const MD_COLLAPSEWHITESPACE: &str = "CollapseWhiteSpace";
pub const MD_DEFAULTPROPERTY: &str = "DefaultProperty";
pub const MD_DEPRECATED: &str = "Deprecated";
pub const MD_EFFECT: &str = "Effect";
pub const MD_EMBED: &str = "Embed";
pub const MD_EVENT: &str = "Event";
pub const MD_FRAME: &str = "Frame";
pub const MD_HOSTCOMPONENT: &str = "HostComponent";
pub const MD_ICONFILE: &str = "IconFile";
pub const MD_IMXMLOBJECT: &str = "IMXMLObject";
pub const MD_INSPECTABLE: &str = "Inspectable";
pub const MD_INSTANCETYPE: &str = "InstanceType";
pub const MD_MANAGED: &str = "Managed";
pub const MD_MIXIN: &str = "Mixin";
pub const MD_NONCOMMITTINGCHANGEEVENT: &str = "NonCommittingChangeEvent";
pub const MD_PERCENTPROXY: &str = "PercentProxy";
pub const MD_REMOTECLASS: &str = "RemoteClass";
pub const MD_REQUIRESLICENSE: &str = "RequiresLicense";
pub const MD_RESOURCEBUNDLE: &str = "ResourceBundle";
pub const MD_RICHTEXTCONTENT: &str = "RichTextContent";
pub const MD_STYLE: &str = "Style";
pub const MD_SWF: &str = "SWF";
pub const MD_TRANSIENT: &str = "Transient";
pub const MD_SKINSTATE: &str = "SkinState";
pub const MD_EXCLUDE: &str = "Exclude";
pub const MD_SKINPART: &str = "SkinPart";
pub const MD_ALTERNATIVE: &str = "Alternative";
pub const MD_DISCOURAGEDFORPROFILE: &str = "DiscouragedForProfile";
pub const MDPARAM_BINDABLE_EVENT: &str = "event";
pub const MDPARAM_TYPE: &str = "type";
pub const MDPARAM_DESTINATION: &str = "destination";
pub const MDPARAM_MODE: &str = "mode";
pub const MDPARAM_STYLE_FORMAT_COLOR: &str = "Color";
pub const MDPARAM_INSPECTABLE_FORMAT_COLOR: &str = "Color";
pub const MDPARAM_PROPERTY_CHANGE: &str = "propertyChange";
pub const MDPARAM_MANAGED_MODE_HIERARCHICAL: &str = "hierarchical";
pub const MDPARAM_MANAGED_MODE_ASSOCIATION: &str = "association";
pub const MDPARAM_MANAGED_MODE_MANUAL: &str = "manual";
pub const GRAPHICS_MASK: &str = "mask";
pub const GRAPHICS_DEFINITION_NAME: &str = "name";
pub const NULL: &str = "null";
pub const UNDEFINED: &str = "undefined";
pub const SPARK_TEXT_ATTRIBUTE: &str = "text";
pub const BINDABLE_PROPERTY_TEMPLATE: &str = "BindableProperty.vm";
pub const CLASSDEF_TEMPLATE: &str = "ClassDef.vm";
pub const CLASSDEF_LIB_TEMPLATE: &str = "ClassDefLib.vm";
pub const EMBED_CLASS_TEMPLATE: &str = "EmbedClass.vm";
pub const FONTFACERULES_TEMPLATE: &str = "FontFaceRules.vm";
pub const INTERFACE_DEF_TEMPLATE: &str = "InterfaceDef.vm";
pub const MANAGED_PROPERTY_TEMPLATE: &str = "ManagedProperty.vm";
pub const SKINCLASS_TEMPLATE: &str = "SkinClass.vm";
pub const STYLEDEF_TEMPLATE: &str = "StyleDef.vm";
pub const STYLE_LIBRARY_TEMPLATE: &str = "StyleLibrary.vm";
pub const STYLE_MODULE_TEMPLATE: &str = "StyleModule.vm";
pub const WATCHER_SETUP_UTIL_TEMPLATE: &str = "WatcherSetupUtil.vm";

pub struct StandardDefs {
    pub namespace_mx_internal: String,
    pub namespace_mx_internal_dot: String,
    pub interface_iflexmodule_dot: String,
    pub interface_iflexmodulefactory_dot: String,
    pub interface_ibindingclient_dot: String,
    pub interface_istylemanager2_dot: String,
    pub interface_ichildlist: String,
    pub interface_icontainer: String,
    pub interface_ideferredinstance: String,
    pub interface_ideferredinstantiationuicomponent: String,
    pub interface_ieventdispatcher: String,
    pub interface_igraphicelement: String,
    pub interface_ifactory: String,
    pub interface_iflexdisplayobject: String,
    pub interface_iflexmodule: String,
    pub interface_ifocusmanagercontainer: String,
    pub interface_iinvalidating: String,
    pub interface_ilayoutmanagerclient: String,
    pub interface_imanaged: String,
    pub interface_imoduleinfo: String,
    pub interface_imxmlobject: String,
    pub interface_inavigatorcontent: String,
    pub interface_ioverride: String,
    pub interface_ipropertychangenotifier: String,
    pub interface_irawchildrencontainer: String,
    pub interface_isimplestyleclient: String,
    pub interface_istateclient2: String,
    pub interface_istyleclient: String,
    pub interface_isystemmanager: String,
    pub interface_itransientdeferredinstance: String,
    pub interface_iuicomponent: String,
    pub interface_ivisualelement: String,
    pub interface_ivisualelementcontainer: String,
    pub class_crossdomainrslitem_dot: String,
    pub class_embeddedfontregistry_dot: String,
    pub class_flexversion_dot: String,
    pub class_effectmanager_dot: String,
    pub class_propertychangeevent_dot: String,
    pub class_request_dot: String,
    pub class_resourcebundle_dot: String,
    pub class_resourcemanager_dot: String,
    pub class_repeater_dot: String,
    pub class_stylemanager_dot: String,
    pub class_stylemanagerimpl_dot: String,
    pub class_systemmanagerchildmanager_dot: String,
    pub class_textfieldfactory_dot: String,
    pub class_singleton_dot: String,
    pub class_abstractservice: String,
    pub class_accordion: String,
    pub class_additems: String,
    pub class_applicationdomain: String,
    pub class_bindingmanager: String,
    pub class_classfactory: String,
    pub class_cssstyledeclaration: String,
    pub class_deferredinstancefromclass: String,
    pub class_deferredinstancefromfunction: String,
    pub class_downloadprogressbar: String,
    pub class_effect: String,
    pub class_event: String,
    pub class_eventdispatcher: String,
    pub class_flexevent: String,
    pub class_flexsprite: String,
    pub class_spark_radiobuttongroup: String,
    pub class_itemscomponent: String,
    pub class_loaderconfig: String,
    pub class_managed: String,
    pub class_moduleevent: String,
    pub class_modulemanager: String,
    pub class_namespaceutil: String,
    pub class_objectproxy: String,
    pub class_preloader: String,
    pub class_propertychangeevent: String,
    pub class_radiobuttongroup: String,
    pub class_repeater: String,
    pub class_seteventhandler: String,
    pub class_setproperty: String,
    pub class_setstyle: String,
    pub class_state: String,
    pub class_styleevent: String,
    pub class_stylemanager: String,
    pub class_systemchildrenlist: String,
    pub class_systemmanager: String,
    pub class_systemrawchildrenlist: String,
    pub class_uicomponent: String,
    pub class_uicomponentdescriptor: String,
    pub class_uidutil: String,
    pub class_viewstack: String,
    pub class_xmlutil: String,
    standard_mxml_imports: std::cell::RefCell<Option<JHashSet<String>>>,
}

impl StandardDefs {
    fn new() -> StandardDefs {
        StandardDefs {
            namespace_mx_internal: "mx.core:mx_internal".to_string(),
            namespace_mx_internal_dot: "mx.core.mx_internal".to_string(),
            interface_iflexmodule_dot: "mx.core.IFlexModule".to_string(),
            interface_iflexmodulefactory_dot: "mx.core.IFlexModuleFactory".to_string(),
            interface_ibindingclient_dot: "mx.binding.IBindingClient".to_string(),
            interface_istylemanager2_dot: "mx.styles.IStyleManager2".to_string(),
            interface_ichildlist: "mx.core:IChildList".to_string(),
            interface_icontainer: "mx.core:IContainer".to_string(),
            interface_ideferredinstance: "mx.core:IDeferredInstance".to_string(),
            interface_ideferredinstantiationuicomponent: "mx.core:IDeferredInstantiationUIComponent".to_string(),
            interface_ieventdispatcher: "flash.events:IEventDispatcher".to_string(),
            interface_igraphicelement: "spark.core:IGraphicElement".to_string(),
            interface_ifactory: "mx.core:IFactory".to_string(),
            interface_iflexdisplayobject: "mx.core:IFlexDisplayObject".to_string(),
            interface_iflexmodule: "mx.core:IFlexModule".to_string(),
            interface_ifocusmanagercontainer: "mx.managers:IFocusManagerContainer".to_string(),
            interface_iinvalidating: "mx.core:IInvalidating".to_string(),
            interface_ilayoutmanagerclient: "mx.managers:ILayoutManagerClient".to_string(),
            interface_imanaged: "mx.data:IManaged".to_string(),
            interface_imoduleinfo: "mx.modules:IModuleInfo".to_string(),
            interface_imxmlobject: "mx.core:IMXMLObject".to_string(),
            interface_inavigatorcontent: "mx.core:INavigatorContent".to_string(),
            interface_ioverride: "mx.states:IOverride".to_string(),
            interface_ipropertychangenotifier: "mx.core:IPropertyChangeNotifier".to_string(),
            interface_irawchildrencontainer: "mx.core:IRawChildrenContainer".to_string(),
            interface_isimplestyleclient: "mx.styles:ISimpleStyleClient".to_string(),
            interface_istateclient2: "mx.core:IStateClient2".to_string(),
            interface_istyleclient: "mx.styles:IStyleClient".to_string(),
            interface_isystemmanager: "mx.managers:ISystemManager".to_string(),
            interface_itransientdeferredinstance: "mx.core:ITransientDeferredInstance".to_string(),
            interface_iuicomponent: "mx.core:IUIComponent".to_string(),
            interface_ivisualelement: "mx.core:IVisualElement".to_string(),
            interface_ivisualelementcontainer: "mx.core:IVisualElementContainer".to_string(),
            class_crossdomainrslitem_dot: "mx.core.CrossDomainRSLItem".to_string(),
            class_embeddedfontregistry_dot: "mx.core.EmbeddedFontRegistry".to_string(),
            class_flexversion_dot: "mx.core.FlexVersion".to_string(),
            class_effectmanager_dot: "mx.effects.EffectManager".to_string(),
            class_propertychangeevent_dot: "mx.events.PropertyChangeEvent".to_string(),
            class_request_dot: "mx.events.Request".to_string(),
            class_resourcebundle_dot: "mx.resources.ResourceBundle".to_string(),
            class_resourcemanager_dot: "mx.resources.ResourceManager".to_string(),
            class_repeater_dot: "mx.core.Repeater".to_string(),
            class_stylemanager_dot: "mx.styles.StyleManager".to_string(),
            class_stylemanagerimpl_dot: "mx.styles.StyleManagerImpl".to_string(),
            class_systemmanagerchildmanager_dot: "mx.managers.systemClasses.ChildManager".to_string(),
            class_textfieldfactory_dot: "mx.core.TextFieldFactory".to_string(),
            class_singleton_dot: "mx.core.Singleton".to_string(),
            class_abstractservice: "mx.rpc:AbstractService".to_string(),
            class_accordion: "mx.containers:Accordion".to_string(),
            class_additems: "mx.states:AddItems".to_string(),
            class_applicationdomain: "flash.system:ApplicationDomain".to_string(),
            class_bindingmanager: "mx.binding:BindingManager".to_string(),
            class_classfactory: "mx.core:ClassFactory".to_string(),
            class_cssstyledeclaration: "mx.styles:CSSStyleDeclaration".to_string(),
            class_deferredinstancefromclass: "mx.core:DeferredInstanceFromClass".to_string(),
            class_deferredinstancefromfunction: "mx.core:DeferredInstanceFromFunction".to_string(),
            class_downloadprogressbar: "mx.preloaders:DownloadProgressBar".to_string(),
            class_effect: "mx.effects:Effect".to_string(),
            class_event: "flash.events:Event".to_string(),
            class_eventdispatcher: "flash.events:EventDispatcher".to_string(),
            class_flexevent: "mx.events:FlexEvent".to_string(),
            class_flexsprite: "mx.core:FlexSprite".to_string(),
            class_spark_radiobuttongroup: "spark.components:RadioButtonGroup".to_string(),
            class_itemscomponent: "spark.components:SkinnableContainer".to_string(),
            class_loaderconfig: "mx.messaging.config:LoaderConfig".to_string(),
            class_managed: "mx.data.utils:Managed".to_string(),
            class_moduleevent: "mx.modules:ModuleEvent".to_string(),
            class_modulemanager: "mx.modules:ModuleManager".to_string(),
            class_namespaceutil: "mx.rpc.xml:NamespaceUtil".to_string(),
            class_objectproxy: "mx.utils:ObjectProxy".to_string(),
            class_preloader: "mx.preloaders:Preloader".to_string(),
            class_propertychangeevent: "mx.events:PropertyChangeEvent".to_string(),
            class_radiobuttongroup: "mx.controls:RadioButtonGroup".to_string(),
            class_repeater: "mx.core:Repeater".to_string(),
            class_seteventhandler: "mx.states:SetEventHandler".to_string(),
            class_setproperty: "mx.states:SetProperty".to_string(),
            class_setstyle: "mx.states:SetStyle".to_string(),
            class_state: "mx.states:State".to_string(),
            class_styleevent: "mx.events:StyleEvent".to_string(),
            class_stylemanager: "mx.styles:StyleManager".to_string(),
            class_systemchildrenlist: "mx.managers:SystemChildrenList".to_string(),
            class_systemmanager: "mx.managers:SystemManager".to_string(),
            class_systemrawchildrenlist: "mx.managers:SystemRawChildrenList".to_string(),
            class_uicomponent: "mx.core:UIComponent".to_string(),
            class_uicomponentdescriptor: "mx.core:UIComponentDescriptor".to_string(),
            class_uidutil: "mx.utils:UIDUtil".to_string(),
            class_viewstack: "mx.containers:ViewStack".to_string(),
            class_xmlutil: "mx.utils:XMLUtil".to_string(),
            standard_mxml_imports: Default::default(),
        }
    }

    /// `getStandardDefs(frameworkType)`: the one `StandardDefs2006`.
    pub fn get_standard_defs(_framework_type: &str) -> Rc<StandardDefs> {
        thread_local! {
            static DEFS: Rc<StandardDefs> = Rc::new(StandardDefs::new());
        }
        DEFS.with(|d| d.clone())
    }

    pub fn get_binding_package(&self) -> &'static str {
        PACKAGE_MX_BINDING
    }
    pub fn get_container_package(&self) -> &'static str {
        PACKAGE_MX_CONTAINERS
    }
    pub fn get_core_package(&self) -> &'static str {
        PACKAGE_MX_CORE
    }
    pub fn get_controls_package(&self) -> &'static str {
        PACKAGE_MX_CONTROLS
    }
    pub fn get_data_package(&self) -> &'static str {
        PACKAGE_MX_DATA
    }
    pub fn get_data_utils_package(&self) -> &'static str {
        PACKAGE_MX_DATA_UTILS
    }
    pub fn get_effects_package(&self) -> &'static str {
        PACKAGE_MX_EFFECTS
    }
    pub fn get_events_package(&self) -> &'static str {
        PACKAGE_MX_EVENTS
    }
    pub fn get_managers_package(&self) -> &'static str {
        PACKAGE_MX_MANAGERS
    }
    pub fn get_messaging_config_package(&self) -> &'static str {
        PACKAGE_MX_MESSAGING_CONFIG
    }
    pub fn get_modules_package(&self) -> &'static str {
        PACKAGE_MX_MODULES
    }
    pub fn get_preloaders_package(&self) -> &'static str {
        PACKAGE_MX_PRELOADERS
    }
    pub fn get_resources_package(&self) -> &'static str {
        PACKAGE_MX_RESOURCES
    }
    pub fn get_rpc_package(&self) -> &'static str {
        PACKAGE_MX_RPC
    }
    pub fn get_rpc_xml_package(&self) -> &'static str {
        PACKAGE_MX_RPC_XML
    }
    pub fn get_states_package(&self) -> &'static str {
        PACKAGE_MX_STATES
    }
    pub fn get_styles_package(&self) -> &'static str {
        PACKAGE_MX_STYLES
    }
    pub fn get_utils_package(&self) -> &'static str {
        PACKAGE_MX_UTILS
    }

    pub fn get_bindable_property_template(&self) -> &'static str {
        BINDABLE_PROPERTY_TEMPLATE
    }
    pub fn get_class_def_template(&self) -> &'static str {
        CLASSDEF_TEMPLATE
    }
    pub fn get_class_def_lib_template(&self) -> &'static str {
        CLASSDEF_LIB_TEMPLATE
    }
    pub fn get_embed_class_template(&self) -> &'static str {
        EMBED_CLASS_TEMPLATE
    }
    pub fn get_font_face_rules_template(&self) -> &'static str {
        FONTFACERULES_TEMPLATE
    }
    pub fn get_interface_def_template(&self) -> &'static str {
        INTERFACE_DEF_TEMPLATE
    }
    pub fn get_managed_property_template(&self) -> &'static str {
        MANAGED_PROPERTY_TEMPLATE
    }
    pub fn get_skin_class_template(&self) -> &'static str {
        SKINCLASS_TEMPLATE
    }
    pub fn get_style_def_template(&self) -> &'static str {
        STYLEDEF_TEMPLATE
    }
    pub fn get_style_library_template(&self) -> &'static str {
        STYLE_LIBRARY_TEMPLATE
    }
    pub fn get_style_module_template(&self) -> &'static str {
        STYLE_MODULE_TEMPLATE
    }
    pub fn get_watcher_setup_util_template(&self) -> &'static str {
        WATCHER_SETUP_UTIL_TEMPLATE
    }

    /// `getStandardMxmlImports()`.
    pub fn get_standard_mxml_imports(&self) -> JHashSet<String> {
        let mut cache = self.standard_mxml_imports.borrow_mut();
        if cache.is_none() {
            let mut s = JHashSet::new();
            s.insert(name_formatter::to_dot_star(self.get_styles_package()));
            s.insert(name_formatter::to_dot_star(self.get_binding_package()));
            s.insert(name_formatter::to_dot(&self.namespace_mx_internal));
            s.insert(name_formatter::to_dot(&self.interface_ideferredinstance));
            s.insert(name_formatter::to_dot(&self.interface_ifactory));
            s.insert(self.interface_iflexmodulefactory_dot.clone());
            s.insert(name_formatter::to_dot(&self.interface_ipropertychangenotifier));
            s.insert(name_formatter::to_dot(&self.class_classfactory));
            s.insert(name_formatter::to_dot(&self.class_deferredinstancefromclass));
            s.insert(name_formatter::to_dot(&self.class_deferredinstancefromfunction));
            *cache = Some(s);
        }
        cache.clone().unwrap()
    }

    /// `getSplitStandardMxmlImports()`.
    pub fn get_split_standard_mxml_imports(&self) -> JHashMap<String, Vec<String>> {
        let mut m = JHashMap::new();
        for i in self.get_standard_mxml_imports().iter() {
            m.insert(i.clone(), i.split('.').map(str::to_string).collect());
        }
        m
    }

    /// `getImports()`: the watcher imports.
    pub fn get_imports(&self) -> Vec<String> {
        let c = self.get_core_package();
        let b = self.get_binding_package();
        vec![
            format!("{c}.IFlexModuleFactory"),
            format!("{b}.ArrayElementWatcher"),
            format!("{b}.FunctionReturnWatcher"),
            format!("{b}.IWatcherSetupUtil2"),
            format!("{b}.PropertyWatcher"),
            format!("{b}.RepeaterComponentWatcher"),
            format!("{b}.RepeaterItemWatcher"),
            format!("{b}.StaticPropertyWatcher"),
            format!("{b}.XMLWatcher"),
            format!("{b}.Watcher"),
        ]
    }

    /// `getConvertedTagName(node)`.
    pub fn get_converted_tag_name(&self, node: &NodeRef) -> String {
        let n = node.borrow();
        if n.is_operation_node() {
            "WebServiceOperation".to_string()
        } else if n.is_method_node() {
            "RemoteObjectOperation".to_string()
        } else {
            n.get_local_part().to_string()
        }
    }

    pub fn is_container(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_icontainer)) || t.is_assignable_to_name(Some(&self.class_repeater))
    }

    pub fn is_navigator_content(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_inavigatorcontent)) || t.is_assignable_to_name(Some(&self.class_repeater))
    }

    pub fn is_iflex_module(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_iflexmodule))
    }

    pub fn is_iui_component(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_iuicomponent))
    }

    pub fn is_items_component(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.class_itemscomponent))
    }

    pub fn is_halo_navigator(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.class_accordion)) || t.is_assignable_to_name(Some(&self.class_viewstack))
    }

    pub fn is_spark_graphic(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_igraphicelement))
    }

    pub fn is_simple_style_component(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_isimplestyleclient))
    }

    /// Type equality, not assignability.
    pub fn is_ifactory(&self, t: &Type) -> bool {
        t.get_name() == self.interface_ifactory
    }

    pub fn is_ideferred_instance(&self, t: &Type) -> bool {
        let n = t.get_name();
        n == self.interface_ideferredinstance || n == self.interface_itransientdeferredinstance
    }

    pub fn is_itransient_deferred_instance(&self, t: &Type) -> bool {
        t.get_name() == self.interface_itransientdeferredinstance
    }

    pub fn is_instance_generator(&self, t: &Type) -> bool {
        self.is_ifactory(t) || self.is_ideferred_instance(t)
    }

    pub fn is_repeater(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.class_repeater))
    }

    pub fn is_state(&self, cls: &str) -> bool {
        self.class_state == cls
    }

    /// `isRepeater(String)`.
    pub fn is_repeater_name(&self, cls: &str) -> bool {
        self.class_repeater == cls
    }

    pub fn get_xml_backing_class_name(&self, e4x: bool) -> &'static str {
        if e4x {
            CLASS_XML
        } else {
            CLASS_XMLNODE
        }
    }

    pub fn is_iui_component_with_id_property(&self, t: &Type) -> bool {
        t.is_assignable_to_name(Some(&self.interface_ideferredinstantiationuicomponent))
    }
}

pub const DEFAULT_AS3_METADATA: [&str; 5] = [MD_BINDABLE, MD_MANAGED, MD_CHANGEEVENT, MD_NONCOMMITTINGCHANGEEVENT, MD_TRANSIENT];

fn jset(items: &[&str]) -> JHashSet<String> {
    let mut s = JHashSet::new();
    for i in items {
        s.insert(i.to_string());
    }
    s
}

/// `implicitImports`.
pub fn implicit_imports() -> JHashSet<String> {
    jset(&[
        "flash.accessibility.*",
        "flash.debugger.*",
        "flash.display.*",
        "flash.errors.*",
        "flash.events.*",
        "flash.external.*",
        "flash.filters.*",
        "flash.geom.*",
        "flash.media.*",
        "flash.net.*",
        "flash.printing.*",
        "flash.profiler.*",
        "flash.system.*",
        "flash.text.*",
        "flash.ui.*",
        "flash.utils.*",
        "flash.xml.*",
    ])
}

/// `airOnlyImplicitImports`.
pub fn air_only_implicit_imports() -> JHashSet<String> {
    jset(&["flash.data.*", "flash.desktop.*", "flash.filesystem.*", "flash.html.*", "flash.html.script.*"])
}

pub const SPARK_TEXT_TAGS: [&str; 13] = ["RichEditableText", "RichText", "TextArea", "TextInput", "a", "br", "div", "img", "p", "span", "tcy", "tab", "TextFlow"];
pub const SPARK_TEXT_CONTENT_PROPERTIES: [&str; 3] = ["content", "mxmlChildren", "text"];

pub fn is_spark_text_tag(s: &str) -> bool {
    SPARK_TEXT_TAGS.contains(&s)
}

pub fn is_spark_text_content_property(s: &str) -> bool {
    SPARK_TEXT_CONTENT_PROPERTIES.contains(&s)
}

const AS3_RESERVED_WORDS: [&str; 48] = [
    "as", "break", "case", "catch", "class", "continue", "default", "do", "else", "extends", "false", "final", "finally", "for", "function", "goto", "if", "implements", "import", "in", "include",
    "instanceof", "interface", "is", "namespace", "new", "null", "package", "private", "protected", "public", "return", "static", "super", "switch", "synchronized", "this", "throw", "transient",
    "true", "try", "typeof", "use", "var", "void", "volatile", "while", "with",
];

pub fn is_reserved_word(s: &str) -> bool {
    AS3_RESERVED_WORDS.contains(&s)
}

const AS3_BUILT_IN_TYPE_NAMES: [&str; 13] = ["String", "Boolean", "Number", "int", "uint", "Function", "Class", CLASS_ARRAY, "Object", "XML", "XMLList", "RegExp", CLASS_VECTOR];

pub fn is_application(t: &Type) -> bool {
    t.is_assignable_to_name(Some(CLASS_APPLICATION)) || t.is_assignable_to_name(Some(CLASS_SPARK_APPLICATION))
}

pub fn is_built_in_type_name(s: &str) -> bool {
    AS3_BUILT_IN_TYPE_NAMES.contains(&s)
}

pub fn is_stage_property(s: &str) -> bool {
    s == "colorCorrection"
}
