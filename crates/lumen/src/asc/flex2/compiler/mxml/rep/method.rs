//! Port of `flex2.compiler.mxml.rep.Method` (a WebService/RemoteObject method).

use std::rc::Rc;

use super::event_handler::EventHandler;
use super::model::ModelRef;

pub struct Method {
    pub name: String,
    pub fault_handler: Option<Rc<EventHandler>>,
    pub result_handler: Option<Rc<EventHandler>>,
    pub concurrency: i32,
    pub my_request: Option<ModelRef>,
    pub show_busy_cursor: Option<String>,
    pub make_objects_bindable: bool,
}

/// The fault/result `EventHandler`s of `Method` and `Operation`.
pub(crate) fn handler(text: Option<&str>, xml_line_number: i32) -> Option<Rc<EventHandler>> {
    text.map(|t| {
        let h = EventHandler::new(None, None, t, None);
        h.set_xml_line_number(xml_line_number);
        Rc::new(h)
    })
}

impl Method {
    pub fn new(name: &str, fault_handler_text: Option<&str>, result_handler_text: Option<&str>, concurrency: i32, xml_line_number: i32, show_busy_cursor: Option<&str>, make_objects_bindable: bool) -> Method {
        Method {
            name: name.to_string(),
            fault_handler: handler(fault_handler_text, xml_line_number),
            result_handler: handler(result_handler_text, xml_line_number),
            concurrency,
            my_request: None,
            show_busy_cursor: show_busy_cursor.map(Into::into),
            make_objects_bindable,
        }
    }
}
