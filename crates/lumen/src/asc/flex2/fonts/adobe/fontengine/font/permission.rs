//! Port of afe.jar `com.adobe.fontengine.font.Permission`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Restricted,
    PreviewAndPrint,
    Editable,
    Installable,
    IllegalValue,
}
