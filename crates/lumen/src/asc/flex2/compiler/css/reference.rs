//! Port of `flex2.compiler.css.Reference`: a `ClassReference(...)` or `PropertyReference(...)` style value.

#[derive(Debug)]
pub struct Reference {
    value: String,
    is_class_reference: bool,
    path: String,
    line_number: i32,
}

impl Reference {
    pub fn new(value: String, is_class_reference: bool, path: String, line_number: i32) -> Reference {
        Reference { value, is_class_reference, path, line_number }
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn get_value(&self) -> &str {
        &self.value
    }

    pub fn is_class_reference(&self) -> bool {
        self.is_class_reference
    }
}
