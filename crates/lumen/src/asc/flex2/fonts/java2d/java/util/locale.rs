//! Port of the JDK 17 `java.util.Locale` values the font code compares; the default and startup locale are en_US,
//! the `user.language`/`user.country` the reference JVM runs with.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    pub language: String,
    pub country: String,
}

impl Locale {
    pub fn new(language: &str, country: &str) -> Locale {
        Locale { language: language.to_string(), country: country.to_string() }
    }

    /// `Locale.US`.
    pub fn us() -> Locale {
        Locale::new("en", "US")
    }

    /// `Locale.ENGLISH`.
    pub fn english() -> Locale {
        Locale::new("en", "")
    }

    /// `Locale.getDefault()`.
    pub fn get_default() -> Locale {
        Locale::us()
    }

    /// `SunToolkit.getStartupLocale()`.
    pub fn startup() -> Locale {
        Locale::us()
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.country.is_empty() {
            write!(f, "{}", self.language)
        } else {
            write!(f, "{}_{}", self.language, self.country)
        }
    }
}
