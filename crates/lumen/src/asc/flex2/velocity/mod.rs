//! Port of Flex's fork of Velocity 1.4 (`modules/thirdparty/velocity`, package `org.apache.flex.forks.velocity`), the
//! template engine mxmlc generates ActionScript with, together with `flex2.compiler.util.VelocityManager`.

pub mod char_stream;
pub mod context;
pub mod directive;
pub mod introspection;
pub mod node;
pub mod parser;
pub mod runtime;
pub mod template;
pub mod token_manager;
pub mod velocity_manager;

#[cfg(test)]
mod tests;

pub use context::{
    EventCartridge, MethodExceptionEventHandler, NullSetEventHandler, ReferenceInsertionEventHandler, VelocityContext,
};
pub use introspection::{JType, JavaException, JavaObject, Method, Value};
pub use template::Template;
pub use velocity_manager::{get_code_gen_context, get_template, get_template_with_lib, register_template_source, Util};

/// What a Java caller of Velocity would catch.
#[derive(Clone, Debug)]
pub enum VelocityError {
    /// `ParseException` / `ParseErrorException`.
    Parse(String),
    /// `MethodInvocationException`.
    MethodInvocation { message: String, method_name: String },
    /// Any other exception, by its `toString()`.
    Runtime(String),
}

impl std::fmt::Display for VelocityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VelocityError::Parse(m) => write!(f, "org.apache.flex.forks.velocity.runtime.parser.ParseException: {m}"),
            VelocityError::MethodInvocation { message, .. } => {
                write!(f, "org.apache.flex.forks.velocity.exception.MethodInvocationException: {message}")
            }
            VelocityError::Runtime(m) => write!(f, "{m}"),
        }
    }
}
