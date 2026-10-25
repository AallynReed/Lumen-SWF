//! Port of `VelocityContext` (with `AbstractContext` and `InternalContextBase`), `InternalContextAdapterImpl`, `VMContext`,
//! `EventCartridge` and the event handler interfaces.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::directive::VMProxyArg;
use super::introspection::Method;
use super::introspection::{JavaException, Value, VelPropertyGet};
use super::runtime::RuntimeInstance;

pub trait ReferenceInsertionEventHandler {
    fn reference_insert(&self, reference: &str, value: Value) -> Value;
}

pub trait NullSetEventHandler {
    fn should_log_on_null_set(&self, lhs: &str, rhs: &str) -> bool;
}

pub trait MethodExceptionEventHandler {
    /// `methodException(claz, method, e)`; `Err` rethrows.
    fn method_exception(&self, class_name: &str, method: &str, e: &JavaException) -> Result<Value, JavaException>;
}

#[derive(Default)]
pub struct EventCartridge {
    pub rieh: Option<Rc<dyn ReferenceInsertionEventHandler>>,
    pub nseh: Option<Rc<dyn NullSetEventHandler>>,
    pub meeh: Option<Rc<dyn MethodExceptionEventHandler>>,
}

impl EventCartridge {
    pub fn reference_insert(&self, reference: &str, value: Value) -> Value {
        match &self.rieh {
            None => value,
            Some(h) => h.reference_insert(reference, value),
        }
    }

    pub fn should_log_on_null_set(&self, lhs: &str, rhs: &str) -> bool {
        match &self.nseh {
            None => true,
            Some(h) => h.should_log_on_null_set(lhs, rhs),
        }
    }

    pub fn method_exception(&self, class_name: &str, method: &str, e: &JavaException) -> Result<Value, JavaException> {
        match &self.meeh {
            None => Err(e.clone()),
            Some(h) => h.method_exception(class_name, method, e),
        }
    }
}

/// `IntrospectionCacheData`: the class it was resolved for and the resolved getter or method.
#[derive(Clone)]
pub enum IcacheThingy {
    Getter(VelPropertyGet),
    Method(Method),
}

#[derive(Clone)]
pub struct IntrospectionCacheData {
    pub context_data: String,
    pub thingy: IcacheThingy,
}

/// `VelocityContext`: a HashMap of names to values, plus the housekeeping of `InternalContextBase`.
#[derive(Default)]
pub struct VelocityContext {
    context: RefCell<HashMap<String, Value>>,
    introspection_cache: RefCell<HashMap<usize, IntrospectionCacheData>>,
    template_name_stack: RefCell<Vec<String>>,
    event_cartridge: RefCell<Option<Rc<EventCartridge>>>,
}

impl VelocityContext {
    pub fn new() -> VelocityContext {
        VelocityContext::default()
    }

    /// `put`; a null value is not stored.
    pub fn put(&self, key: &str, value: Value) -> Value {
        if value.is_null() {
            return Value::Null;
        }
        self.context.borrow_mut().insert(key.to_string(), value).unwrap_or(Value::Null)
    }

    pub fn get(&self, key: &str) -> Value {
        self.context.borrow().get(key).cloned().unwrap_or(Value::Null)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.context.borrow().contains_key(key)
    }

    pub fn remove(&self, key: &str) -> Value {
        self.context.borrow_mut().remove(key).unwrap_or(Value::Null)
    }

    /// `EventCartridge.attachToContext`.
    pub fn attach_event_cartridge(&self, ec: Rc<EventCartridge>) -> Option<Rc<EventCartridge>> {
        self.event_cartridge.replace(Some(ec))
    }

    pub fn event_cartridge(&self) -> Option<Rc<EventCartridge>> {
        self.event_cartridge.borrow().clone()
    }

    pub(crate) fn push_current_template_name(&self, s: &str) {
        self.template_name_stack.borrow_mut().push(s.to_string());
    }

    pub(crate) fn pop_current_template_name(&self) {
        self.template_name_stack.borrow_mut().pop();
    }

    pub(crate) fn current_template_name(&self) -> String {
        self.template_name_stack.borrow().last().cloned().unwrap_or_else(|| "<undef>".to_string())
    }
}

/// `VMContext`: a macro invocation's arguments over the caller's context.
pub struct VMContext<'a> {
    vmproxyhash: RefCell<HashMap<String, VMProxyArg>>,
    localcontext: RefCell<HashMap<String, Value>>,
    wrapped: Ica<'a>,
    inner: &'a VelocityContext,
    rsvc: &'a RuntimeInstance,
}

impl<'a> VMContext<'a> {
    pub fn new(inner: Ica<'a>, rsvc: &'a RuntimeInstance) -> VMContext<'a> {
        VMContext {
            vmproxyhash: RefCell::new(HashMap::new()),
            localcontext: RefCell::new(HashMap::new()),
            inner: inner.base(),
            wrapped: inner,
            rsvc,
        }
    }

    pub fn add_vm_proxy_arg(&self, vmpa: VMProxyArg) {
        let key = vmpa.context_reference.clone();
        self.vmproxyhash.borrow_mut().insert(key, vmpa);
    }
}

/// `InternalContextAdapter`: the user context or a macro's `VMContext` over it.
#[derive(Clone, Copy)]
pub enum Ica<'a> {
    Base(&'a VelocityContext),
    Vm(&'a VMContext<'a>),
}

impl<'a> Ica<'a> {
    /// `getBaseContext()`.
    pub fn base(&self) -> &'a VelocityContext {
        match self {
            Ica::Base(c) => c,
            Ica::Vm(v) => v.inner,
        }
    }

    pub fn put(&self, key: &str, value: Value) -> Result<Value, super::VelocityError> {
        match self {
            Ica::Base(c) => Ok(c.put(key, value)),
            Ica::Vm(v) => {
                let vmpa = v.vmproxyhash.borrow().get(key).cloned();
                if let Some(vmpa) = vmpa {
                    return Ok(vmpa.set_object(v.rsvc, v.wrapped, value));
                }
                if v.localcontext.borrow().contains_key(key) {
                    return Ok(v.localcontext.borrow_mut().insert(key.to_string(), value).unwrap_or(Value::Null));
                }
                Ok(v.inner.put(key, value))
            }
        }
    }

    pub fn get(&self, key: &str) -> Result<Value, super::VelocityError> {
        match self {
            Ica::Base(c) => Ok(c.get(key)),
            Ica::Vm(v) => {
                let vmpa = v.vmproxyhash.borrow().get(key).cloned();
                if let Some(vmpa) = vmpa {
                    return vmpa.get_object(v.rsvc, v.wrapped);
                }
                let o = v.localcontext.borrow().get(key).cloned().unwrap_or(Value::Null);
                if o.is_null() {
                    Ok(v.inner.get(key))
                } else {
                    Ok(o)
                }
            }
        }
    }

    pub fn remove(&self, key: &str) {
        match self {
            Ica::Base(c) => {
                c.remove(key);
            }
            Ica::Vm(v) => {
                v.vmproxyhash.borrow_mut().remove(key);
            }
        }
    }

    pub fn current_template_name(&self) -> String {
        self.base().current_template_name()
    }

    pub fn icache_get(&self, key: usize) -> Option<IntrospectionCacheData> {
        self.base().introspection_cache.borrow().get(&key).cloned()
    }

    pub fn icache_put(&self, key: usize, o: IntrospectionCacheData) {
        self.base().introspection_cache.borrow_mut().insert(key, o);
    }

    pub fn event_cartridge(&self) -> Option<Rc<EventCartridge>> {
        self.base().event_cartridge()
    }
}
