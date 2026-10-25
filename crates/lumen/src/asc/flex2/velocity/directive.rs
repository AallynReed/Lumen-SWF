//! Port of the runtime directives: `Foreach`, `Macro` (`processAndRegister`), `Literal`, `VelocimacroProxy` and `VMProxyArg`
//! (Flex's fork: arguments are evaluated lazily from the call-site nodes); `#include` and `#parse` are only recognized.

use std::rc::Rc;

use super::context::{Ica, VMContext};
use super::introspection::{get_iterator, Info, Value};
use super::node::{Node, Sink};
use super::parser::{JJTWORD, JJT_NODE_NAME};
use super::runtime::{MacroEntry, RuntimeInstance, COUNTER_INITIAL_VALUE, COUNTER_NAME};
use super::VelocityError;

pub enum DirectiveInst {
    Foreach(Foreach),
    Macro,
    Literal(String),
    Include,
    Parse,
    Proxy(VelocimacroProxy),
}

pub struct Foreach {
    counter_name: String,
    counter_initial_value: i32,
    element_key: String,
    uber_info: Info,
}

/// The directive instance `ASTDirective.init` creates and initializes.
pub fn create(
    rsvc: &RuntimeInstance,
    cx: Ica,
    node: &Rc<Node>,
    name: &str,
) -> Result<Option<DirectiveInst>, VelocityError> {
    let d = match name {
        "foreach" => {
            let first = node.children[0].first_token_image.clone().unwrap_or_default();
            DirectiveInst::Foreach(Foreach {
                counter_name: COUNTER_NAME.to_string(),
                counter_initial_value: COUNTER_INITIAL_VALUE,
                element_key: crate::asc::util::jstr::substring_from(&first, 1),
                // Directive.getLine()/getColumn() are still 0: setLocation runs after init.
                uber_info: Info::new(&cx.current_template_name(), 0, 0),
            })
        }
        "macro" => {
            process_and_register(rsvc, node, &cx.current_template_name())?;
            DirectiveInst::Macro
        }
        "literal" => DirectiveInst::Literal(node.children[0].literal()?),
        "include" => DirectiveInst::Include,
        "parse" => DirectiveInst::Parse,
        _ => match rsvc.get_velocimacro(name, &cx.current_template_name()) {
            None => return Ok(None),
            Some(mut p) => {
                p.init(rsvc, node);
                DirectiveInst::Proxy(p)
            }
        },
    };
    Ok(Some(d))
}

/// `Macro.processAndRegister`.
fn process_and_register(rs: &RuntimeInstance, node: &Rc<Node>, source_template: &str) -> Result<(), VelocityError> {
    let num_args = node.children.len();
    if num_args < 2 {
        rs.error(&format!("#macro error : Velocimacro must have name as 1st argument to #macro(). #args = {num_args}"));
        return Err(VelocityError::Runtime("First argument to #macro() must be  macro name.".to_string()));
    }
    let first_type = node.children[0].id;
    if first_type != JJTWORD {
        return Err(VelocityError::Runtime(format!(
            "First argument to #macro() must be a token without surrounding ' or \", which specifies the macro name.  Currently it is a {}",
            JJT_NODE_NAME[first_type as usize]
        )));
    }
    let mut args: Vec<String> = Vec::new();
    for i in 0..num_args - 1 {
        let mut a = node.children[i].first_token_image.clone().unwrap_or_else(|| "null".to_string());
        if i > 0 && a.starts_with('$') {
            a = crate::asc::util::jstr::substring_from(&a, 1);
        }
        args.push(a);
    }
    let name = args.remove(0);
    rs.add_velocimacro(&name, Some(node.children[num_args - 1].clone()), args, source_template);
    Ok(())
}

impl DirectiveInst {
    pub fn render(
        &self,
        rsvc: &RuntimeInstance,
        cx: Ica,
        w: &mut Sink,
        node: &Rc<Node>,
    ) -> Result<bool, VelocityError> {
        match self {
            DirectiveInst::Foreach(f) => f.render(rsvc, cx, w, node),
            DirectiveInst::Macro => Ok(true),
            DirectiveInst::Literal(t) => {
                w.push_str(t);
                Ok(true)
            }
            DirectiveInst::Include | DirectiveInst::Parse => {
                rsvc.error("#include and #parse are not supported by this port of Velocity");
                Ok(false)
            }
            DirectiveInst::Proxy(p) => p.render(rsvc, cx, w),
        }
    }
}

impl Foreach {
    fn render(&self, rsvc: &RuntimeInstance, cx: Ica, w: &mut Sink, node: &Rc<Node>) -> Result<bool, VelocityError> {
        let list_object = node.children[2].value(rsvc, cx)?;
        if list_object.is_null() {
            return Ok(false);
        }
        let Some(mut i) = get_iterator(rsvc, &list_object, &self.uber_info) else { return Ok(false) };
        let mut counter = self.counter_initial_value;
        let o = cx.get(&self.element_key)?;
        let ctr = cx.get(&self.counter_name)?;
        let mut pos = 0;
        while let Some(item) = i.next(&mut pos) {
            cx.put(&self.counter_name, Value::Integer(counter))?;
            cx.put(&self.element_key, item)?;
            node.children[3].render(rsvc, cx, w)?;
            counter += 1;
        }
        if !ctr.is_null() {
            cx.put(&self.counter_name, ctr)?;
        } else {
            cx.remove(&self.counter_name);
        }
        if !o.is_null() {
            cx.put(&self.element_key, o)?;
        } else {
            cx.remove(&self.element_key);
        }
        Ok(true)
    }
}

/// `VMProxyArg` as Flex's fork builds it: an argument name bound to the call-site node, evaluated on each read.
#[derive(Clone)]
pub struct VMProxyArg {
    pub context_reference: String,
    node_tree: Rc<Node>,
}

impl VMProxyArg {
    pub fn get_object(&self, rsvc: &RuntimeInstance, cx: Ica) -> Result<Value, VelocityError> {
        match self.node_tree.value(rsvc, cx) {
            Err(VelocityError::MethodInvocation { message: m, .. }) => {
                rsvc.error(&format!(
                    "VMProxyArg.getObject() : method invocation error getting value : org.apache.flex.forks.velocity.exception.MethodInvocationException: {m}"
                ));
                Ok(Value::Null)
            }
            r => r,
        }
    }

    /// `setObject`: the fork never sets the argument type, so every argument is a "constant".
    pub fn set_object(&self, rsvc: &RuntimeInstance, _cx: Ica, _o: Value) -> Value {
        rsvc.error(&format!(
            "VMProxyArg.setObject() : Programmer error : I am a constant!  No setting! : {} / null",
            self.context_reference
        ));
        Value::Null
    }
}

pub struct VelocimacroProxy {
    macro_name: String,
    argument_names_array: Vec<String>,
    args: Vec<Option<VMProxyArg>>,
    node_tree: Option<Rc<Node>>,
}

impl VelocimacroProxy {
    pub fn new(me: &MacroEntry, _namespace: &str) -> VelocimacroProxy {
        VelocimacroProxy {
            macro_name: me.macroname.clone(),
            argument_names_array: me.argarray.clone(),
            args: vec![None; me.argarray.len()],
            node_tree: me.node_tree.clone(),
        }
    }

    fn init(&mut self, rsvc: &RuntimeInstance, node: &Rc<Node>) {
        let i = node.children.len();
        if self.argument_names_array.len() != i {
            rsvc.error(&format!(
                "VM #{}: error : too {} arguments to macro. Wanted {} got {}",
                self.macro_name,
                if self.argument_names_array.len() > i { "few" } else { "many" },
                self.argument_names_array.len(),
                i
            ));
            return;
        }
        for (k, name) in self.argument_names_array.iter().enumerate() {
            self.args[k] = Some(VMProxyArg { context_reference: name.clone(), node_tree: node.children[k].clone() });
        }
    }

    fn render(&self, rsvc: &RuntimeInstance, cx: Ica, w: &mut Sink) -> Result<bool, VelocityError> {
        let Some(tree) = &self.node_tree else {
            rsvc.error(&format!("VM error : {}. Null AST", self.macro_name));
            return Ok(true);
        };
        let vmc = VMContext::new(cx, rsvc);
        for a in &self.args {
            match a {
                Some(a) => vmc.add_vm_proxy_arg(a.clone()),
                None => {
                    rsvc.error(&format!(
                        "VelocimacroProxy.render() : exception VM = #{}() : java.lang.NullPointerException",
                        self.macro_name
                    ));
                    return Ok(true);
                }
            }
        }
        match tree.render(rsvc, Ica::Vm(&vmc), w) {
            Ok(_) => {}
            Err(e @ VelocityError::MethodInvocation { .. }) => return Err(e),
            Err(e) => rsvc.error(&format!("VelocimacroProxy.render() : exception VM = #{}() : {}", self.macro_name, e)),
        }
        Ok(true)
    }
}
