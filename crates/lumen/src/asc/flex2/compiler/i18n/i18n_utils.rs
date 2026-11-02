//! Port of the `flex2.compiler.i18n.I18nUtils` pieces mxmlc uses: the bundle class suffix and the
//! `_CompiledResourceBundleInfo` class.

use super::LINE_SEP;

pub const CLASS_SUFFIX: &str = "_properties";
pub const COMPILED_RESOURCE_BUNDLE_INFO: &str = "_CompiledResourceBundleInfo";

fn quoted(items: &[String]) -> String {
    items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
}

/// `codegenCompiledResourceBundleInfo(locales, bundleNames)`.
pub fn codegen_compiled_resource_bundle_info(locales: &[String], bundle_names: &[String]) -> String {
    let l = LINE_SEP;
    [
        "package ", "", l, "{", l, l, "[ExcludeClass]", l, l, "public class ", COMPILED_RESOURCE_BUNDLE_INFO, l, "{", l,
        "    public static function get compiledLocales():Array /* of String */", l, "    {", l,
        "        return ", &format!("[ {} ]", quoted(locales)), ";", l, "    }", l, l,
        "    public static function get compiledResourceBundleNames():Array /* of String */", l, "    {", l,
        "        return ", &format!("[ {} ]", quoted(bundle_names)), ";", l, "    }", l, "}", l, l, "}", l,
    ]
    .concat()
}

/// `getClassName(locale, bundleName)`.
pub fn get_class_name(locale: &str, bundle_name: &str) -> String {
    format!("{locale}${bundle_name}{CLASS_SUFFIX}")
}

/// `bundleNameFromClassName(className)`: `en_US$core_properties` -> `core`.
pub fn bundle_name_from_class_name(class_name: &str) -> String {
    let start = class_name.find('$').map_or(0, |i| i + 1);
    class_name[start..class_name.len() - CLASS_SUFFIX.len()].to_string()
}
