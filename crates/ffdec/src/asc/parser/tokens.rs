//! Port of `macromedia/asc/parser/Tokens.java`.

pub const FIRST_TOKEN: i32 = -1;
pub const EOS_TOKEN: i32 = FIRST_TOKEN - 0;
pub const MINUS_TOKEN: i32 = FIRST_TOKEN - 1;
pub const MINUSMINUS_TOKEN: i32 = MINUS_TOKEN - 1;
pub const NOT_TOKEN: i32 = MINUSMINUS_TOKEN - 1;
pub const NOTEQUALS_TOKEN: i32 = NOT_TOKEN - 1;
pub const STRICTNOTEQUALS_TOKEN: i32 = NOTEQUALS_TOKEN - 1;
pub const MODULUS_TOKEN: i32 = STRICTNOTEQUALS_TOKEN - 1;
pub const MODULUSASSIGN_TOKEN: i32 = MODULUS_TOKEN - 1;
pub const BITWISEAND_TOKEN: i32 = MODULUSASSIGN_TOKEN - 1;
pub const LOGICALAND_TOKEN: i32 = BITWISEAND_TOKEN - 1;
pub const LOGICALANDASSIGN_TOKEN: i32 = LOGICALAND_TOKEN - 1;
pub const BITWISEANDASSIGN_TOKEN: i32 = LOGICALANDASSIGN_TOKEN - 1;
pub const LEFTPAREN_TOKEN: i32 = BITWISEANDASSIGN_TOKEN - 1;
pub const RIGHTPAREN_TOKEN: i32 = LEFTPAREN_TOKEN - 1;
pub const MULT_TOKEN: i32 = RIGHTPAREN_TOKEN - 1;
pub const MULTASSIGN_TOKEN: i32 = MULT_TOKEN - 1;
pub const COMMA_TOKEN: i32 = MULTASSIGN_TOKEN - 1;
pub const DOT_TOKEN: i32 = COMMA_TOKEN - 1;
pub const DOUBLEDOT_TOKEN: i32 = DOT_TOKEN - 1;
pub const TRIPLEDOT_TOKEN: i32 = DOUBLEDOT_TOKEN - 1;
pub const DOTLESSTHAN_TOKEN: i32 = TRIPLEDOT_TOKEN - 1;
pub const DIV_TOKEN: i32 = DOTLESSTHAN_TOKEN - 1;
pub const DIVASSIGN_TOKEN: i32 = DIV_TOKEN - 1;
pub const COLON_TOKEN: i32 = DIVASSIGN_TOKEN - 1;
pub const DOUBLECOLON_TOKEN: i32 = COLON_TOKEN - 1;
pub const SEMICOLON_TOKEN: i32 = DOUBLECOLON_TOKEN - 1;
pub const QUESTIONMARK_TOKEN: i32 = SEMICOLON_TOKEN - 1;
pub const ATSIGN_TOKEN: i32 = QUESTIONMARK_TOKEN - 1;
pub const LEFTBRACKET_TOKEN: i32 = ATSIGN_TOKEN - 1;
pub const RIGHTBRACKET_TOKEN: i32 = LEFTBRACKET_TOKEN - 1;
pub const BITWISEXOR_TOKEN: i32 = RIGHTBRACKET_TOKEN - 1;
pub const LOGICALXOR_TOKEN: i32 = BITWISEXOR_TOKEN - 1;
pub const LOGICALXORASSIGN_TOKEN: i32 = LOGICALXOR_TOKEN - 1;
pub const BITWISEXORASSIGN_TOKEN: i32 = LOGICALXORASSIGN_TOKEN - 1;
pub const LEFTBRACE_TOKEN: i32 = BITWISEXORASSIGN_TOKEN - 1;
pub const BITWISEOR_TOKEN: i32 = LEFTBRACE_TOKEN - 1;
pub const LOGICALOR_TOKEN: i32 = BITWISEOR_TOKEN - 1;
pub const LOGICALORASSIGN_TOKEN: i32 = LOGICALOR_TOKEN - 1;
pub const BITWISEORASSIGN_TOKEN: i32 = LOGICALORASSIGN_TOKEN - 1;
pub const RIGHTBRACE_TOKEN: i32 = BITWISEORASSIGN_TOKEN - 1;
pub const BITWISENOT_TOKEN: i32 = RIGHTBRACE_TOKEN - 1;
pub const PLUS_TOKEN: i32 = BITWISENOT_TOKEN - 1;
pub const PLUSPLUS_TOKEN: i32 = PLUS_TOKEN - 1;
pub const PLUSASSIGN_TOKEN: i32 = PLUSPLUS_TOKEN - 1;
pub const LESSTHAN_TOKEN: i32 = PLUSASSIGN_TOKEN - 1;
pub const LEFTSHIFT_TOKEN: i32 = LESSTHAN_TOKEN - 1;
pub const LEFTSHIFTASSIGN_TOKEN: i32 = LEFTSHIFT_TOKEN - 1;
pub const LESSTHANOREQUALS_TOKEN: i32 = LEFTSHIFTASSIGN_TOKEN - 1;
pub const ASSIGN_TOKEN: i32 = LESSTHANOREQUALS_TOKEN - 1;
pub const MINUSASSIGN_TOKEN: i32 = ASSIGN_TOKEN - 1;
pub const EQUALS_TOKEN: i32 = MINUSASSIGN_TOKEN - 1;
pub const STRICTEQUALS_TOKEN: i32 = EQUALS_TOKEN - 1;
pub const GREATERTHAN_TOKEN: i32 = STRICTEQUALS_TOKEN - 1;
pub const GREATERTHANOREQUALS_TOKEN: i32 = GREATERTHAN_TOKEN - 1;
pub const RIGHTSHIFT_TOKEN: i32 = GREATERTHANOREQUALS_TOKEN - 1;
pub const RIGHTSHIFTASSIGN_TOKEN: i32 = RIGHTSHIFT_TOKEN - 1;
pub const UNSIGNEDRIGHTSHIFT_TOKEN: i32 = RIGHTSHIFTASSIGN_TOKEN - 1;
pub const UNSIGNEDRIGHTSHIFTASSIGN_TOKEN: i32 = UNSIGNEDRIGHTSHIFT_TOKEN - 1;
pub const ABSTRACT_TOKEN: i32 = UNSIGNEDRIGHTSHIFTASSIGN_TOKEN - 1;
pub const AS_TOKEN: i32 = ABSTRACT_TOKEN - 1;
pub const BREAK_TOKEN: i32 = AS_TOKEN - 1;
pub const CASE_TOKEN: i32 = BREAK_TOKEN - 1;
pub const CATCH_TOKEN: i32 = CASE_TOKEN - 1;
pub const CLASS_TOKEN: i32 = CATCH_TOKEN - 1;
pub const CONST_TOKEN: i32 = CLASS_TOKEN - 1;
pub const CONTINUE_TOKEN: i32 = CONST_TOKEN - 1;
pub const DEBUGGER_TOKEN: i32 = CONTINUE_TOKEN - 1;
pub const DEFAULT_TOKEN: i32 = DEBUGGER_TOKEN - 1;
pub const DELETE_TOKEN: i32 = DEFAULT_TOKEN - 1;
pub const DO_TOKEN: i32 = DELETE_TOKEN - 1;
pub const ELSE_TOKEN: i32 = DO_TOKEN - 1;
pub const ENUM_TOKEN: i32 = ELSE_TOKEN - 1;
pub const EXTENDS_TOKEN: i32 = ENUM_TOKEN - 1;
pub const FALSE_TOKEN: i32 = EXTENDS_TOKEN - 1;
pub const FINAL_TOKEN: i32 = FALSE_TOKEN - 1;
pub const FINALLY_TOKEN: i32 = FINAL_TOKEN - 1;
pub const FOR_TOKEN: i32 = FINALLY_TOKEN - 1;
pub const FUNCTION_TOKEN: i32 = FOR_TOKEN - 1;
pub const GET_TOKEN: i32 = FUNCTION_TOKEN - 1;
pub const GOTO_TOKEN: i32 = GET_TOKEN - 1;
pub const IF_TOKEN: i32 = GOTO_TOKEN - 1;
pub const IMPLEMENTS_TOKEN: i32 = IF_TOKEN - 1;
pub const IMPORT_TOKEN: i32 = IMPLEMENTS_TOKEN - 1;
pub const IN_TOKEN: i32 = IMPORT_TOKEN - 1;
pub const INCLUDE_TOKEN: i32 = IN_TOKEN - 1;
pub const INSTANCEOF_TOKEN: i32 = INCLUDE_TOKEN - 1;
pub const INTERFACE_TOKEN: i32 = INSTANCEOF_TOKEN - 1;
pub const IS_TOKEN: i32 = INTERFACE_TOKEN - 1;
pub const NAMESPACE_TOKEN: i32 = IS_TOKEN - 1;
pub const CONFIG_TOKEN: i32 = NAMESPACE_TOKEN - 1;
pub const NATIVE_TOKEN: i32 = CONFIG_TOKEN - 1;
pub const NEW_TOKEN: i32 = NATIVE_TOKEN - 1;
pub const NULL_TOKEN: i32 = NEW_TOKEN - 1;
pub const PACKAGE_TOKEN: i32 = NULL_TOKEN - 1;
pub const PRIVATE_TOKEN: i32 = PACKAGE_TOKEN - 1;
pub const PROTECTED_TOKEN: i32 = PRIVATE_TOKEN - 1;
pub const PUBLIC_TOKEN: i32 = PROTECTED_TOKEN - 1;
pub const RETURN_TOKEN: i32 = PUBLIC_TOKEN - 1;
pub const SET_TOKEN: i32 = RETURN_TOKEN - 1;
pub const STATIC_TOKEN: i32 = SET_TOKEN - 1;
pub const SUPER_TOKEN: i32 = STATIC_TOKEN - 1;
pub const SWITCH_TOKEN: i32 = SUPER_TOKEN - 1;
pub const SYNCHRONIZED_TOKEN: i32 = SWITCH_TOKEN - 1;
pub const THIS_TOKEN: i32 = SYNCHRONIZED_TOKEN - 1;
pub const THROW_TOKEN: i32 = THIS_TOKEN - 1;
pub const THROWS_TOKEN: i32 = THROW_TOKEN - 1;
pub const TRANSIENT_TOKEN: i32 = THROWS_TOKEN - 1;
pub const TRUE_TOKEN: i32 = TRANSIENT_TOKEN - 1;
pub const TRY_TOKEN: i32 = TRUE_TOKEN - 1;
pub const TYPEOF_TOKEN: i32 = TRY_TOKEN - 1;
pub const USE_TOKEN: i32 = TYPEOF_TOKEN - 1;
pub const VAR_TOKEN: i32 = USE_TOKEN - 1;
pub const VOID_TOKEN: i32 = VAR_TOKEN - 1;
pub const VOLATILE_TOKEN: i32 = VOID_TOKEN - 1;
pub const WHILE_TOKEN: i32 = VOLATILE_TOKEN - 1;
pub const WITH_TOKEN: i32 = WHILE_TOKEN - 1;
pub const IDENTIFIER_TOKEN: i32 = WITH_TOKEN - 1;
pub const NUMBERLITERAL_TOKEN: i32 = IDENTIFIER_TOKEN - 1;
pub const REGEXPLITERAL_TOKEN: i32 = NUMBERLITERAL_TOKEN - 1;
pub const STRINGLITERAL_TOKEN: i32 = REGEXPLITERAL_TOKEN - 1;
pub const NEGMINLONGLITERAL_TOKEN: i32 = STRINGLITERAL_TOKEN - 1;
pub const XMLLITERAL_TOKEN: i32 = NEGMINLONGLITERAL_TOKEN - 1;
pub const XMLPART_TOKEN: i32 = XMLLITERAL_TOKEN - 1;
pub const XMLMARKUP_TOKEN: i32 = XMLPART_TOKEN - 1;
pub const XMLTEXT_TOKEN: i32 = XMLMARKUP_TOKEN - 1;
pub const XMLTAGENDEND_TOKEN: i32 = XMLTEXT_TOKEN - 1;
pub const XMLTAGSTARTEND_TOKEN: i32 = XMLTAGENDEND_TOKEN - 1;
pub const ATTRIBUTEIDENTIFIER_TOKEN: i32 = XMLTAGSTARTEND_TOKEN - 1;
pub const DOCCOMMENT_TOKEN: i32 = ATTRIBUTEIDENTIFIER_TOKEN - 1;
pub const BLOCKCOMMENT_TOKEN: i32 = DOCCOMMENT_TOKEN - 1;
pub const SLASHSLASHCOMMENT_TOKEN: i32 = BLOCKCOMMENT_TOKEN - 1;
pub const EOL_TOKEN: i32 = SLASHSLASHCOMMENT_TOKEN - 1;
pub const EMPTY_TOKEN: i32 = EOL_TOKEN - 1;
pub const ERROR_TOKEN: i32 = EMPTY_TOKEN - 1;
pub const LAST_TOKEN: i32 = EMPTY_TOKEN - 1;

pub static TOKEN_CLASS_NAMES: [&str; 137] = [
    "<unused index>",
    "end of program",
    "minus",
    "minusminus",
    "not",
    "notequals",
    "strictnotequals",
    "modulus",
    "modulusassign",
    "bitwiseand",
    "logicaland",
    "logicalandassign",
    "bitwiseandassign",
    "leftparen",
    "rightparen",
    "mult",
    "multassign",
    "comma",
    "dot",
    "doubledot",
    "tripledot",
    "dotlessthan",
    "div",
    "divassign",
    "colon",
    "doublecolon",
    "semicolon",
    "questionmark",
    "atsign",
    "leftbracket",
    "rightbracket",
    "bitwisexor",
    "logicalxor",
    "logicalxorassign",
    "bitwisexorassign",
    "leftbrace",
    "bitwiseor",
    "logicalor",
    "logicalorassign",
    "bitwiseorassign",
    "rightbrace",
    "bitwisenot",
    "plus",
    "plusplus",
    "plusassign",
    "lessthan",
    "leftshift",
    "leftshiftassign",
    "lessthanorequals",
    "assign",
    "minusassign",
    "equals",
    "strictequals",
    "greaterthan",
    "greaterthanorequals",
    "rightshift",
    "rightshiftassign",
    "unsignedrightshift",
    "unsignedrightshiftassign",
    "abstract",
    "as",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "for",
    "function",
    "get",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "include",
    "instanceof",
    "interface",
    "is",
    "namespace",
    "config",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "typeof",
    "use",
    "var",
    "void",
    "volatile",
    "while",
    "with",
    "identifier",
    "numberliteral",
    "regexpliteral",
    "stringliteral",
    "negminlongliteral",
    "xmlliteral",
    "xmlpart",
    "xmlmarkup",
    "xmltext",
    "xmltagendend",
    "xmltagstartend",
    "attributeidentifier",
    "docComment",
    "blockComment",
    "slashslashcomment",
    "end of line",
    "<empty>",
    "<error>",
    "abbrev_mode",
    "full_mode",
];

pub static TOKEN_TO_STRING: [&str; 137] = [
    "",
    "",
    "-",
    "--",
    "!",
    "!=",
    "!==",
    "%",
    "%=",
    "&",
    "&&",
    "&&=",
    "&=",
    "(",
    ")",
    "*",
    "*=",
    ",",
    ".",
    "..",
    "...",
    ".<",
    "/",
    "/=",
    ":",
    "::",
    ";",
    "?",
    "@",
    "[",
    "]",
    "^",
    "^^",
    "^^=",
    "^=",
    "{",
    "|",
    "||",
    "||=",
    "|=",
    "}",
    "~",
    "+",
    "++",
    "+=",
    "<",
    "<<",
    "<<=",
    "<=",
    "=",
    "-=",
    "==",
    "===",
    ">",
    ">=",
    ">>",
    ">>=",
    ">>>",
    ">>>=",
    "abstract",
    " as ",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "for",
    "function",
    "get",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "include",
    "instanceof",
    "interface",
    " is ",
    "namespace",
    "config",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public ",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "typeof",
    "use namespace ",
    "var",
    "void",
    "volatile",
    "while",
    "with",
    "",
    "",
    "",
    "",
    "",
    "",
    "{",
    "",
    "",
    "/>",
    "</",
    "",
    "",
    "/*",
    "//",
    "",
    "",
    "",
    "",
    "",
];
