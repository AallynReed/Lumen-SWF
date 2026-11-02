//! Port of `flex2.compiler.as3.binding.BindableInfo`, the `GenerativeClassInfo` subclass data for `[Bindable]`.

#[derive(Default)]
pub struct BindableInfo {
    needs_to_implement_ievent_dispatcher: bool,
    needs_static_event_dispatcher: bool,
    requires_static_event_dispatcher: bool,
}

impl BindableInfo {
    pub fn needs_additional_interfaces(&self) -> bool {
        self.needs_to_implement_ievent_dispatcher
    }

    pub fn get_needs_to_implement_ievent_dispatcher(&self) -> bool {
        self.needs_to_implement_ievent_dispatcher
    }

    pub fn get_needs_static_event_dispatcher(&self) -> bool {
        self.needs_static_event_dispatcher
    }

    pub fn get_requires_static_event_dispatcher(&self) -> bool {
        self.requires_static_event_dispatcher
    }

    pub fn set_needs_to_implement_ievent_dispatcher(&mut self, b: bool) {
        self.needs_to_implement_ievent_dispatcher = b;
    }

    pub fn set_needs_static_event_dispatcher(&mut self, b: bool) {
        self.needs_static_event_dispatcher = b;
    }

    pub fn set_requires_static_event_dispatcher(&mut self, b: bool) {
        self.requires_static_event_dispatcher = b;
    }
}
