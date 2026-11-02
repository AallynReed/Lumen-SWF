//! Port of `flex2.compiler.mxml.rep.Operation` (a WebService/RemoteObject operation).

use std::rc::Rc;

use super::event_handler::EventHandler;
use super::method::handler;
use super::model::ModelRef;

pub const CONCURRENCY_MULTIPLE: i32 = 0;
pub const CONCURRENCY_SINGLE: i32 = 1;
pub const CONCURRENCY_LAST: i32 = 2;

pub struct Operation {
    pub name: String,
    pub fault_handler: Option<Rc<EventHandler>>,
    pub result_handler: Option<Rc<EventHandler>>,
    pub concurrency: i32,
    pub result_format: Option<String>,
    pub my_request: Option<ModelRef>,
    pub show_busy_cursor: Option<String>,
    pub make_objects_bindable: bool,
}

impl Operation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(name: &str, fault_handler_text: Option<&str>, result_handler_text: Option<&str>, concurrency: i32, result_format: Option<&str>, xml_line_number: i32, show_busy_cursor: Option<&str>, make_objects_bindable: bool) -> Operation {
        Operation {
            name: name.to_string(),
            fault_handler: handler(fault_handler_text, xml_line_number),
            result_handler: handler(result_handler_text, xml_line_number),
            concurrency,
            result_format: result_format.map(Into::into),
            my_request: None,
            show_busy_cursor: show_busy_cursor.map(Into::into),
            make_objects_bindable,
        }
    }
}
