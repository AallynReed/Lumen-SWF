//! Port of `Template` (`initDocument`, `merge`) and `SerializedTemplateFactory`: a template is parsed and initialized by a
//! build-time runtime, then keeps only the state Java serialization keeps, as the `.vms` resources in mxmlc.jar do.

use std::cell::RefCell;
use std::rc::Rc;

use super::context::{Ica, VelocityContext};
use super::node::{self, Node};
use super::runtime::RuntimeInstance;
use super::VelocityError;

pub struct Template {
    name: String,
    data: Rc<Node>,
    rsvc: RefCell<Option<Rc<RuntimeInstance>>>,
}

impl Template {
    /// `SerializedTemplateFactory.main` (`Velocity.getTemplate` in the `build` runtime) followed by `load`.
    pub fn build(source: &str, name: &str, build: &Rc<RuntimeInstance>) -> Result<Template, VelocityError> {
        let data = node::parse_tree(source, name, build)?;
        let t = Template { name: name.to_string(), data, rsvc: RefCell::new(Some(build.clone())) };
        t.init_document()?;
        t.data.strip_transient();
        t.rsvc.replace(None);
        Ok(t)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_runtime_services(&self, rsvc: &Rc<RuntimeInstance>) {
        self.rsvc.replace(Some(rsvc.clone()));
    }

    pub fn data(&self) -> &Rc<Node> {
        &self.data
    }

    pub fn init_document(&self) -> Result<(), VelocityError> {
        let rsvc = self
            .rsvc
            .borrow()
            .clone()
            .ok_or_else(|| VelocityError::Runtime("java.lang.NullPointerException".into()))?;
        let cx = VelocityContext::new();
        cx.push_current_template_name(&self.name);
        let r = self.data.init(&rsvc, Ica::Base(&cx));
        cx.pop_current_template_name();
        r
    }

    /// `merge(context, writer)`, returning what was written.
    pub fn merge(&self, context: &VelocityContext) -> Result<String, VelocityError> {
        let mut w = String::new();
        self.merge_into(context, &mut w).map(|_| w)
    }

    /// `merge` into an existing writer; on error the partial output stays written, as with a Java `Writer`.
    pub fn merge_into(&self, context: &VelocityContext, w: &mut String) -> Result<(), VelocityError> {
        let rsvc = self
            .rsvc
            .borrow()
            .clone()
            .ok_or_else(|| VelocityError::Runtime("java.lang.NullPointerException".into()))?;
        context.push_current_template_name(&self.name);
        let r = self.data.render(&rsvc, Ica::Base(context), w);
        context.pop_current_template_name();
        r.map(|_| ())
    }
}
