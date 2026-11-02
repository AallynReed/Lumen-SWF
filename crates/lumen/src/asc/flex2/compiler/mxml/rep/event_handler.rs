//! Port of `flex2.compiler.mxml.rep.EventHandler`.

use std::cell::{Cell, RefCell};

use super::deps::{Event, Type};
use super::model::ModelRef;

pub struct EventHandler {
    model: Option<ModelRef>,
    event: Option<Event>,
    event_handler_text: String,
    document_function_name: RefCell<Option<String>>,
    state: Option<String>,
    xml_line_number: Cell<i32>,
}

impl EventHandler {
    /// `new EventHandler(model, event, text)` / `new EventHandler(model, event, text, state)`.
    pub fn new(model: Option<ModelRef>, event: Option<Event>, event_handler_text: &str, state: Option<&str>) -> EventHandler {
        EventHandler {
            model,
            event,
            event_handler_text: event_handler_text.to_string(),
            document_function_name: RefCell::new(None),
            state: state.map(Into::into),
            xml_line_number: Cell::new(0),
        }
    }

    pub fn get_model(&self) -> Option<&ModelRef> {
        self.model.as_ref()
    }

    pub fn get_name(&self) -> String {
        self.event.as_ref().expect("NullPointerException").get_name().unwrap_or("null").to_string()
    }

    pub fn get_type(&self) -> Type {
        self.event.as_ref().expect("NullPointerException").get_type().expect("NullPointerException")
    }

    pub fn get_event_handler_text(&self) -> &str {
        &self.event_handler_text
    }

    pub fn get_document_function_name(&self) -> String {
        if self.document_function_name.borrow().is_none() {
            let mut buf = String::from("__");
            build_name_chain(&mut buf, self.model.as_ref());
            buf.push('_');
            buf.push_str(&self.get_name());
            if let Some(state) = &self.state {
                buf.push('_');
                buf.push_str(state);
            }
            *self.document_function_name.borrow_mut() = Some(buf);
        }
        self.document_function_name.borrow().clone().unwrap()
    }

    pub fn get_xml_line_number(&self) -> i32 {
        self.xml_line_number.get()
    }

    pub fn set_xml_line_number(&self, xml_line_number: i32) {
        self.xml_line_number.set(xml_line_number);
    }
}

fn build_name_chain(buf: &mut String, model: Option<&ModelRef>) -> bool {
    let Some(m) = model else { return false };
    match m.get_id() {
        Some(id) => buf.push_str(&id),
        None => {
            let need_sep = build_name_chain(buf, m.get_parent().as_ref());
            if need_sep {
                buf.push('_');
            }
            buf.push_str(&m.get_parent_index().unwrap_or_else(|| "null".into()));
        }
    }
    true
}
