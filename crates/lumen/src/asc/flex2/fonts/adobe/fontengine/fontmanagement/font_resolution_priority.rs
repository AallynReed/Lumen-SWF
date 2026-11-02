//! Port of afe.jar `com.adobe.fontengine.fontmanagement.FontResolutionPriority`.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontResolutionPriority {
    /// Also `INTELLIGENT`.
    IntelligentLast,
    IntelligentFirst,
    First,
    Last,
}

impl FontResolutionPriority {
    fn names(&self) -> &'static [&'static str] {
        match self {
            FontResolutionPriority::IntelligentLast => &["intelligent last", "intelligent"],
            FontResolutionPriority::IntelligentFirst => &["intelligent first"],
            FontResolutionPriority::First => &["first"],
            FontResolutionPriority::Last => &["last"],
        }
    }

    pub fn parse(s: &str) -> Option<FontResolutionPriority> {
        [FontResolutionPriority::IntelligentLast, FontResolutionPriority::IntelligentFirst, FontResolutionPriority::First, FontResolutionPriority::Last]
            .into_iter()
            .find(|p| p.names().iter().any(|n| n.eq_ignore_ascii_case(s)))
    }

    pub fn to_java_string(&self) -> &'static str {
        self.names()[0]
    }
}
