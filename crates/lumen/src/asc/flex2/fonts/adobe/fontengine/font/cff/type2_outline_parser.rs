//! Port of afe.jar `com.adobe.fontengine.font.cff.Type2OutlineParser`.

use crate::asc::flex2::fonts::adobe::fontengine::font::cff::char_strings::CharStrings;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::name_keyed_font::NameKeyedFont;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_hinted_redirector::{HintedBinding, Type2HintedRedirector};
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_parser::Type2Parser;
use crate::asc::flex2::fonts::adobe::fontengine::font::cff::type2_unhinted_redirector::{Type2UnhintedRedirector, UnhintedBinding};
use crate::asc::flex2::fonts::adobe::fontengine::font::hinted_outline_consumer::HintedOutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font::matrix::Matrix;
use crate::asc::flex2::fonts::adobe::fontengine::font::outline_consumer::OutlineConsumer;
use crate::asc::flex2::fonts::adobe::fontengine::font_engine_exception::FontResult;

pub enum Redirector {
    Unhinted(Type2UnhintedRedirector),
    Hinted(Type2HintedRedirector),
}

/// The consumer handed to `parse`; Java casts it to `HintedOutlineConsumer` when the parser is hinted.
pub enum OutlineTarget<'a> {
    Plain(&'a mut dyn OutlineConsumer),
    Hinted(&'a mut dyn HintedOutlineConsumer),
}

pub struct Type2OutlineParser {
    type2_parser: Type2Parser,
    type2_consumer: Redirector,
}

impl Type2OutlineParser {
    pub fn new(bl: bool) -> Type2OutlineParser {
        Type2OutlineParser {
            type2_parser: Type2Parser::new(),
            type2_consumer: if bl { Redirector::Hinted(Type2HintedRedirector::new()) } else { Redirector::Unhinted(Type2UnhintedRedirector::new()) },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn parse(&mut self, cs: &CharStrings, n: i32, lsubrs: Option<&CharStrings>, gsubrs: Option<&CharStrings>, oc: OutlineTarget<'_>, m: &Matrix, nkf: Option<&NameKeyedFont>) -> FontResult<()> {
        match (&mut self.type2_consumer, oc) {
            (Redirector::Unhinted(r), OutlineTarget::Plain(oc)) => {
                r.reset();
                oc.set_matrix(m);
                let mut b = UnhintedBinding { r, oc };
                self.type2_parser.parse(cs, n, lsubrs, gsubrs, &mut b, nkf)
            }
            (Redirector::Unhinted(r), OutlineTarget::Hinted(oc)) => {
                r.reset();
                oc.set_matrix(m);
                let mut b = UnhintedBinding { r, oc };
                self.type2_parser.parse(cs, n, lsubrs, gsubrs, &mut b, nkf)
            }
            (Redirector::Hinted(r), OutlineTarget::Hinted(oc)) => {
                r.reset();
                oc.set_matrix(m);
                let mut b = HintedBinding { r, oc };
                self.type2_parser.parse(cs, n, lsubrs, gsubrs, &mut b, nkf)
            }
            (Redirector::Hinted(_), OutlineTarget::Plain(_)) => panic!("java.lang.ClassCastException: not a HintedOutlineConsumer"),
        }
    }
}
