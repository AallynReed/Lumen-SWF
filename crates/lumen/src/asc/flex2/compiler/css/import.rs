//! Port of `flex2.compiler.css.Import`: equal and hashed by its class name.

use crate::asc::util::jhash::JavaHash;

#[derive(Clone, Debug)]
pub struct Import {
    value: String,
    line_number: i32,
}

impl Import {
    pub fn new(value: String, line_number: i32) -> Import {
        Import { value, line_number }
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_value(&self) -> &str {
        &self.value
    }
}

impl PartialEq for Import {
    fn eq(&self, other: &Import) -> bool {
        self.value == other.value
    }
}

impl Eq for Import {}

impl JavaHash for Import {
    fn java_hash(&self) -> i32 {
        self.value.java_hash()
    }
}
