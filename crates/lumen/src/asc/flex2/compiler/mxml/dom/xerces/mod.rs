//! The Xerces-J 2.9.1 SAX parser `MxmlScanner` gets from `SAXParserFactory` (namespace-aware, non-validating, with a
//! `LexicalHandler`): the callbacks it makes, in order, with the `Locator` position at each.

pub mod messages;
pub mod reader;
pub mod scanner;
pub mod xml_char;

/// The SAX `Locator` at a callback; `available` is false before `startDocument` (no `setDocumentLocator` yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locator {
    pub available: bool,
    pub line: i32,
    pub column: i32,
}

/// One entry of the SAX `Attributes` (`getURI`, `getLocalName`, `getQName`, `getValue`).
#[derive(Clone, Debug)]
pub struct SaxAttr {
    pub uri: String,
    pub local: String,
    pub qname: String,
    pub value: String,
}

/// `ContentHandler`, `LexicalHandler` and `ErrorHandler.fatalError`; an `Err` aborts the parse (a Java `Error`).
pub trait SaxHandler {
    type Error;
    fn start_document(&mut self, loc: Locator) -> Result<(), Self::Error>;
    fn end_document(&mut self, loc: Locator) -> Result<(), Self::Error>;
    fn start_prefix_mapping(&mut self, prefix: &str, uri: &str, loc: Locator) -> Result<(), Self::Error>;
    fn end_prefix_mapping(&mut self, prefix: &str, loc: Locator) -> Result<(), Self::Error>;
    fn start_element(&mut self, uri: &str, local: &str, qname: &str, attrs: &[SaxAttr], loc: Locator) -> Result<(), Self::Error>;
    fn end_element(&mut self, uri: &str, local: &str, qname: &str, loc: Locator) -> Result<(), Self::Error>;
    fn characters(&mut self, text: &str, loc: Locator) -> Result<(), Self::Error>;
    fn processing_instruction(&mut self, target: &str, data: &str, loc: Locator) -> Result<(), Self::Error>;
    fn start_cdata(&mut self, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn end_cdata(&mut self, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn comment(&mut self, _text: &str, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn start_entity(&mut self, _name: &str, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn end_entity(&mut self, _name: &str, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn start_dtd(&mut self, _name: &str, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    fn end_dtd(&mut self, _loc: Locator) -> Result<(), Self::Error> {
        Ok(())
    }
    /// `fatalError(SAXParseException)`, before the parse stops.
    fn fatal_error(&mut self, message: &str, loc: Locator) -> Result<(), Self::Error>;
    /// `EntityResolver.resolveEntity` for the external DTD subset (only with `Options::dtd`); `None` loads nothing.
    fn resolve_entity(&mut self, _public_id: Option<&str>, _system_id: Option<&str>) -> Option<String> {
        None
    }
}

/// Parser features beyond the ones `MxmlScanner` uses.
#[derive(Clone, Copy, Default)]
pub struct Options {
    /// `namespace-prefixes`: report `xmlns` attributes.
    pub namespace_prefixes: bool,
    /// Read the internal subset and the resolved external subset: general entities and attribute defaults.
    pub dtd: bool,
}

/// How a parse ended other than normally.
pub enum ParseFailure<E> {
    /// A fatal error, already passed to `fatal_error`.
    Fatal(String),
    /// An `IOException` thrown out of `SAXParser.parse`.
    Io(String),
    /// The handler's own abort.
    Handler(E),
}

/// `SAXParser.parse(in, handler)`.
pub fn parse<H: SaxHandler>(bytes: Vec<u8>, handler: &mut H) -> Result<(), ParseFailure<H::Error>> {
    parse_with(bytes, handler, Options::default())
}

/// `parse` with extra parser features.
pub fn parse_with<H: SaxHandler>(bytes: Vec<u8>, handler: &mut H, opts: Options) -> Result<(), ParseFailure<H::Error>> {
    match scanner::parse_with(bytes, handler, opts) {
        Ok(()) => Ok(()),
        Err(scanner::Stop::Fatal(m)) => Err(ParseFailure::Fatal(m)),
        Err(scanner::Stop::Io(m)) => Err(ParseFailure::Io(m)),
        Err(scanner::Stop::Handler(e)) => Err(ParseFailure::Handler(e)),
        Err(scanner::Stop::Eof) => Ok(()),
    }
}
