//! Port of ASC 1.0, the ActionScript 3 compiler inside mxmlc (Apache Flex SDK 4.8.0,
//! `modules/asc/src/java`). Module layout follows the Java packages.

pub mod abc;
pub mod emit;
pub mod parser;
pub mod semantics;
pub mod util;
