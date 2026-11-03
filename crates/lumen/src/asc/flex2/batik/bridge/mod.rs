//! Ports of Batik's `bridge` package for static SVG: `BridgeContext`, `BridgeException` and its messages,
//! `UnitProcessor`, `SVGUtilities`, `CSSUtilities`, `ViewBox`, `URIResolver`/`DocumentLoader`, `GVTBuilder` with
//! the graphics node bridges, and `PaintServer` with the gradient bridges.

pub mod bridge_context;
pub mod css_utilities;
pub mod gvt_builder;
pub mod paint_server;
pub mod svg_utilities;
pub mod unit_processor;
pub mod view_box;

use crate::asc::flex2::batik::dom::NodeRef;

pub const ERR_ATTRIBUTE_MISSING: &str = "attribute.missing";
pub const ERR_ATTRIBUTE_VALUE_MALFORMED: &str = "attribute.malformed";
pub const ERR_LENGTH_NEGATIVE: &str = "length.negative";
pub const ERR_CSS_LENGTH_NEGATIVE: &str = "css.length.negative";
pub const ERR_CSS_URI_BAD_TARGET: &str = "css.uri.badTarget";
pub const ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES: &str = "xlink.href.circularDependencies";
pub const ERR_URI_MALFORMED: &str = "uri.malformed";
pub const ERR_URI_BAD_TARGET: &str = "uri.badTarget";
pub const ERR_URI_IO: &str = "uri.io";
pub const ERR_URI_UNSECURE: &str = "uri.unsecure";
pub const ERR_URI_REFERENCE_A_DOCUMENT: &str = "uri.referenceDocument";
pub const ERR_URI_IMAGE_INVALID: &str = "uri.image.invalid";

/// Why building or painting the tree stopped.
#[derive(Debug, Clone)]
pub enum BridgeError {
    /// `BridgeException`: its formatted message.
    Bridge(String),
    /// Another `RuntimeException` (an `IllegalArgumentException` from a paint, ...): its message.
    Runtime(Option<String>),
    /// A part of SVG rendering this port does not reproduce.
    Unsupported(String),
}

pub type BResult<T> = Result<T, BridgeError>;

/// `new BridgeException(e, code, params).getMessage()` (the line number is the `-1` `GVTBuilder` sets).
pub fn bridge_exception(e: &NodeRef, code: &str, params: &[&str]) -> BridgeError {
    let uri = e.get_url().unwrap_or_else(|| "null".into());
    let lname = e.local_name().unwrap_or("null");
    let p = |i: usize| params.get(i).copied().unwrap_or("null");
    let head = format!("{uri}:-1\n");
    let body = match code {
        ERR_ATTRIBUTE_MISSING => format!("The attribute \"{}\" of the element <{lname}> is required", p(0)),
        ERR_ATTRIBUTE_VALUE_MALFORMED => format!("The attribute \"{}\" of the element <{lname}> is invalid", p(0)),
        ERR_LENGTH_NEGATIVE => format!("The attribute \"{}\" of the element <{lname}> can not be negative", p(0)),
        ERR_CSS_LENGTH_NEGATIVE => format!("The CSS property \"{}\" found on the element <{lname}> can not be negative", p(0)),
        ERR_CSS_URI_BAD_TARGET => format!("The URI \"{}\"\nfor a CSS property found on the element <{lname}> is invalid", p(0)),
        ERR_XLINK_HREF_CIRCULAR_DEPENDENCIES => format!("A circular dependency has been detected on the element <{lname}> for the URI:\n\"{}\"", p(0)),
        ERR_URI_MALFORMED => format!("The URI \"{}\"\nspecified on the element <{lname}> is invalid", p(0)),
        ERR_URI_BAD_TARGET => format!("Cannot find the referenced element:\n\"{}\"\nspecified on the element <{lname}> - may be a problem of id", p(0)),
        ERR_URI_IO => format!("An I/O error occured while processing the URI:\n\"{}\"\nspecified on the element <{lname}>", p(0)),
        ERR_URI_UNSECURE => format!("A security exception occured while processing the URI:\n\"{}\"\nspecified on the element <{lname}>", p(0)),
        ERR_URI_REFERENCE_A_DOCUMENT => format!("The URI \"{0}\"\nspecified on the element <{lname}> references an entire document. \nThis is illegal: The element <{lname}> must reference an element\ninside a document.", p(0)),
        ERR_URI_IMAGE_INVALID => format!("The URI \"{}\"\nspecified on the element <{lname}> is invalid", p(0)),
        c => c.to_string(),
    };
    BridgeError::Bridge(head + &body)
}
