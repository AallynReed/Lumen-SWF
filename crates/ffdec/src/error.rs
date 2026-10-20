#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unexpected end of data")]
    Eof,
    #[error("not a SWF file (signature {0:?})")]
    BadSignature(String),
    #[error("SWF header is too short")]
    HeaderTooShort,
    #[error("decryption failed: {0}")]
    Decrypt(String),
    #[error("LZMA: {0}")]
    Lzma(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
