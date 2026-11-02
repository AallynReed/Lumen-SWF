//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2ConsumerDefaultImpl` (the bodies are the trait defaults).

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_consumer::Type2Consumer;

#[derive(Clone, Copy, Debug, Default)]
pub struct Type2ConsumerDefaultImpl;

impl Type2Consumer for Type2ConsumerDefaultImpl {}
