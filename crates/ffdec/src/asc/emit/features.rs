//! Port of `macromedia/asc/embedding/avmplus/Features.java`, with the values its static initialiser computes when none
//! of the `ES4`/`E4X`/`AS3`/`ES3` system properties is set (so AS3, ES4 and E4X are all on), as under mxmlc.

pub const ES4: bool = true;
pub const E4X: bool = true;
pub const AS3: bool = true;
pub const ES3: bool = false;

/// Use `LookupBaseObject` even when the scope holding a variable is known.
pub const LOOKUP_LEXICAL_REFERENCES: bool = false;
pub const SPECIAL_FUNCTION_SYNTAX: bool = false;
/// Compound names (`base.name` registered with the context) are single names; used for package names.
pub const TRANSLATE_COMPOUND_NAMES: bool = true;
pub const COMPILE_TIME_IF: bool = false;
pub const COMPILE_TIME_IF_KEYWORD: &str = "UNUSED";
pub const KEEP_GLOBAL_THIS_EXPLICIT: bool = false;
pub const NO_NESTED_FUNCTIONS: bool = false;
pub const DUPLICATE_FUNCTION_NAME_ERROR: bool = false;
pub const IDENTIFIER_EVAL_PROLOG: bool = true;
pub const IDENTIFIER_EVAL_EPILOG: bool = true;
pub const PASS_ESCAPES_TO_BACKEND: bool = false;
pub const USE_DEFINED_NAME_AS_INTERNAL_NAME: bool = false;
pub const DONT_SET_FUNCTIONS_EXPRESSIONS: bool = false;
pub const USE_STATIC_SEMANTICS: bool = false;
pub const FUTURE_ABC: bool = false;

pub const HAS_NONIDENTFIELDNAMES: bool = true;
pub const HAS_QUALIFIEDIDENTIFIERS: bool = true;
pub const HAS_DESCENDOPERATORS: bool = true;
pub const HAS_FILTEROPERATORS: bool = true;
pub const HAS_ATTRIBUTEIDENTIFIERS: bool = true;
pub const HAS_WILDCARDSELECTOR: bool = true;
pub const HAS_XMLLITERALS: bool = true;
pub const HAS_EXPRESSIONQUALIFIEDIDS: bool = true;
pub const HAS_REGULAREXPRESSIONS: bool = true;
pub const HAS_ACCESSSPECIFIERS: bool = true;
pub const HAS_SUPEREXPRESSIONS: bool = true;
pub const HAS_RESTPARAMETERS: bool = true;
pub const HAS_ISOPERATOR: bool = true;
pub const HAS_ASOPERATOR: bool = true;
pub const HAS_STRICTNOTEQUALS: bool = true;
pub const HAS_LOGICALASSIGNMENT: bool = true;
pub const HAS_SUPERSTATEMENTS: bool = true;
pub const HAS_ATTRIBUTES: bool = true;
pub const HAS_SQUAREBRACKETATTRS: bool = true;
pub const HAS_LABELEDSTATEMENTS: bool = true;
pub const HAS_INCLUDEDIRECTIVES: bool = true;
pub const HAS_IMPORTDIRECTIVES: bool = true;
pub const HAS_USEDIRECTIVES: bool = true;
pub const HAS_HASHPRAGMAS: bool = false;
pub const HAS_CONSTVARIABLES: bool = true;
pub const HAS_CONSTPARAMETERS: bool = false;
pub const HAS_CLASSDEFINITIONS: bool = true;
pub const HAS_COMPOUNDCLASSNAMES: bool = true;
pub const HAS_INTERFACEDEFINITIONS: bool = true;
pub const HAS_NAMESPACEDEFINITIONS: bool = true;
pub const HAS_TYPEDIDENTIFIERS: bool = true;
pub const HAS_ACCESSORDEFINITIONS: bool = true;
pub const HAS_PACKAGEDEFINITIONS: bool = true;
pub const HAS_RUNTIMECOMPILES: bool = true;

/// Language versions for `Context::dialect`.
pub const DIALECT_ES3: i32 = 9;
pub const DIALECT_AS3: i32 = 10;

/// VM targets: Flash 9 and Flash 10.
pub const TARGET_AVM1: i32 = 0;
pub const TARGET_AVM2: i32 = 1;
