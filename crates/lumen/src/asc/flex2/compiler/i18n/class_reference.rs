//! Port of `flex2.compiler.i18n.ClassReference`: a `ClassReference("x.y.Z")` value in a resource bundle.

#[derive(Clone, Debug)]
pub struct ClassReference {
    value: Option<String>,
    line_number: i32,
}

impl ClassReference {
    pub fn new(value: Option<String>, line_number: i32) -> ClassReference {
        ClassReference { value, line_number }
    }

    pub fn get_line_number(&self) -> i32 {
        self.line_number
    }

    pub fn get_value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

impl std::fmt::Display for ClassReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.value.as_deref().unwrap_or("null"))
    }
}
