//! Port of `macromedia.abc.DecoderException`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderException {
    message: String,
}

impl DecoderException {
    pub fn new(message: impl Into<String>) -> DecoderException {
        DecoderException { message: message.into() }
    }

    pub fn get_message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for DecoderException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DecoderException {}
