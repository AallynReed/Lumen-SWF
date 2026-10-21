//! AS3 compiler: FFDec's `ActionScript3Parser`, `AVM2SourceGenerator` and `AbcIndexing`.

pub mod assign;
pub mod ast;
pub mod gen;
pub mod index;
pub mod ins;
pub mod lexer;
pub mod pack;
pub mod parser;
pub mod pool;
pub mod resolve;
pub mod traits;

pub use pack::replace_script;

/// A compile error on a 1-based line.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for CompileError {}

pub type R<T> = Result<T, CompileError>;
