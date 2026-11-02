//! Port of afe.jar `com.adobe.fontengine.font.postscript.SeacPhase`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeacPhase {
    SeacNone,
    SeacBase,
    SeacAccentPreMove,
    SeacAccentPostMove,
}
