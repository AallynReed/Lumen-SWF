//! Serialization of ActionScript 1/2 actions to SWF byte streams.

use crate::io::Writer;

use super::{Action, ActionList, Charset, Op, Value};

impl Action {
    pub fn write(&self, w: &mut Writer, _cs: Charset) {
        if matches!(self.op, Op::End) {
            w.u8(0);
            return;
        }
        w.u8(self.code);
        if self.code < 0x80 {
            return;
        }

        let mut cw = Writer::new();
        match &self.op {
            Op::Simple | Op::End => {}
            Op::GotoFrame(f) => cw.u16(*f),
            Op::GetUrl { url, target } => {
                cw.string(url);
                cw.string(target);
            }
            Op::WaitForFrame { frame, skip } => {
                cw.u16(*frame);
                cw.u8(*skip as u8);
            }
            Op::SetTarget(s) | Op::GoToLabel(s) => cw.string(s),
            Op::Push { values, .. } => {
                for v in values {
                    match v {
                        Value::Str(s) => {
                            cw.u8(0);
                            cw.string(s);
                        }
                        Value::Float(f) => {
                            cw.u8(1);
                            cw.bytes(&f.to_le_bytes());
                        }
                        Value::Null => cw.u8(2),
                        Value::Undefined => cw.u8(3),
                        Value::Register(r) => {
                            cw.u8(4);
                            cw.u8(*r);
                        }
                        Value::Bool(b) => {
                            cw.u8(5);
                            cw.u8(if *b { 1 } else { 0 });
                        }
                        Value::Double(d) => {
                            cw.u8(6);
                            cw.bytes(&d.to_le_bytes());
                        }
                        Value::Int(i) => {
                            cw.u8(7);
                            cw.bytes(&(*i as i32).to_le_bytes());
                        }
                        Value::Constant(c) => {
                            if *c < 256 {
                                cw.u8(8);
                                cw.u8(*c as u8);
                            } else {
                                cw.u8(9);
                                cw.u16(*c);
                            }
                        }
                    }
                }
            }
            Op::Jump { offset, .. } | Op::If { offset } => cw.i16(*offset as i16),
            Op::GetUrl2 { load_variables, load_target, method } => {
                let b = (*method << 6) | ((*load_target as u8) << 1) | (*load_variables as u8);
                cw.u8(b);
            }
            Op::GotoFrame2 { scene_bias_flag, play, scene_bias } => {
                let b = ((*scene_bias_flag as u8) << 1) | (*play as u8);
                cw.u8(b);
                if *scene_bias_flag {
                    cw.u16(*scene_bias);
                }
            }
            Op::WaitForFrame2 { skip } => cw.u8(*skip as u8),
            Op::ConstantPool(pool) => {
                cw.u16(pool.len() as u16);
                for s in pool {
                    cw.string(s);
                }
            }
            Op::DefineFunction { name, params, code_size } => {
                cw.string(name);
                cw.u16(params.len() as u16);
                for p in params {
                    cw.string(p);
                }
                cw.u16(*code_size as u16);
            }
            Op::With { code_size } => cw.u16(*code_size as u16),
            Op::StoreRegister(r) => cw.u8(*r),
            Op::DefineFunction2(f) => {
                cw.string(&f.name);
                cw.u16(f.params.len() as u16);
                cw.u8(f.register_count);
                let flags: u16 = (f.preload_parent as u16) << 7
                    | (f.preload_root as u16) << 6
                    | (f.suppress_super as u16) << 5
                    | (f.preload_super as u16) << 4
                    | (f.suppress_arguments as u16) << 3
                    | (f.preload_arguments as u16) << 2
                    | (f.suppress_this as u16) << 1
                    | (f.preload_this as u16)
                    | ((f.preload_global as u16) << 10);
                cw.u16(flags);
                for (reg, name) in &f.params {
                    cw.u8(*reg);
                    cw.string(name);
                }
                cw.u16(f.code_size as u16);
            }
            Op::Try(t) => {
                let flags = (t.catch_in_register as u8) << 2
                    | (t.finally_flag as u8) << 1
                    | (t.catch_flag as u8);
                cw.u8(flags);
                cw.u16(t.try_size as u16);
                cw.u16(t.catch_size as u16);
                cw.u16(t.finally_size as u16);
                if t.catch_in_register {
                    cw.u8(t.catch_register);
                } else {
                    cw.string(t.catch_name.as_deref().unwrap_or(""));
                }
            }
            Op::StrictMode(m) => cw.u8(*m),
            Op::Unknown(d) => cw.bytes(d),
        }
        w.u16(cw.data.len() as u16);
        w.bytes(&cw.data);
    }
}

impl ActionList {
    pub fn write(&self, w: &mut Writer) {
        for a in &self.actions {
            a.write(w, self.charset);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write(&mut w);
        w.data
    }
}
