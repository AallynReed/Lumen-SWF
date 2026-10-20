//! Action serialization, as FFDec's `Action.getBytes`.

use crate::io::Writer;

use super::{utf8_bytes, Action, ActionList, Charset, Op, Value, CP1252_HIGH};

pub fn encode(cs: Charset, s: &str) -> Vec<u8> {
    match cs {
        Charset::Utf8 => utf8_bytes(s),
        Charset::Windows1252 => s
            .chars()
            .map(|c| match c as u32 {
                v if v < 0x80 || (0xa0..0x100).contains(&v) => v as u8,
                v => CP1252_HIGH.iter().position(|&h| h as u32 == v).map_or(b'?', |i| 0x80 + i as u8),
            })
            .collect(),
    }
}

fn string(w: &mut Writer, cs: Charset, s: &str) {
    w.bytes(&encode(cs, s));
    w.u8(0);
}

/// SWF DOUBLE: the high 32-bit word first.
fn double(w: &mut Writer, d: f64) {
    let b = d.to_le_bytes();
    w.bytes(&b[4..]);
    w.bytes(&b[..4]);
}

fn value(w: &mut Writer, cs: Charset, v: &Value) {
    match v {
        Value::Str(s) => {
            w.u8(0);
            string(w, cs, s);
        }
        Value::Float(f) => {
            w.u8(1);
            w.bytes(&f.to_le_bytes());
        }
        Value::Null => w.u8(2),
        Value::Undefined => w.u8(3),
        Value::Register(r) => {
            w.u8(4);
            w.u8(*r);
        }
        Value::Bool(b) => {
            w.u8(5);
            w.u8(*b as u8);
        }
        Value::Double(d) => {
            w.u8(6);
            double(w, *d);
        }
        Value::Int(i) if i32::try_from(*i).is_err() => {
            w.u8(6);
            double(w, *i as f64);
        }
        Value::Int(i) => {
            w.u8(7);
            w.bytes(&(*i as i32).to_le_bytes());
        }
        Value::Constant(c) if *c < 256 => {
            w.u8(8);
            w.u8(*c as u8);
        }
        Value::Constant(c) => {
            w.u8(9);
            w.u16(*c);
        }
    }
}

impl Action {
    pub fn write(&self, w: &mut Writer, cs: Charset) {
        if matches!(self.op, Op::End) {
            w.u8(0);
            return;
        }
        w.u8(self.code);
        if self.code < 0x80 {
            return;
        }
        let mut c = Writer::new();
        let u8s = Charset::Utf8;
        match &self.op {
            Op::Simple | Op::End => {}
            Op::GotoFrame(f) => c.u16(*f),
            Op::GetUrl { url, target } => {
                string(&mut c, cs, url);
                string(&mut c, cs, target);
            }
            Op::WaitForFrame { frame, skip } => {
                c.u16(*frame);
                c.u8(*skip as u8);
            }
            Op::SetTarget(s) | Op::GoToLabel(s) => string(&mut c, cs, s),
            Op::Push { values, .. } => values.iter().for_each(|v| value(&mut c, cs, v)),
            Op::Jump { offset, .. } | Op::If { offset } => c.i16(*offset as i16),
            Op::GetUrl2 { load_variables, load_target, method } => c.u8((*load_variables as u8) << 7 | (*load_target as u8) << 6 | method & 3),
            Op::GotoFrame2 { scene_bias_flag, play, scene_bias } => {
                c.u8((*scene_bias_flag as u8) << 1 | *play as u8);
                if *scene_bias_flag {
                    c.u16(*scene_bias);
                }
            }
            Op::WaitForFrame2 { skip } => c.u8(*skip as u8),
            Op::ConstantPool(pool) => {
                c.u16(pool.len() as u16);
                pool.iter().for_each(|s| string(&mut c, cs, s));
            }
            Op::DefineFunction { name, params, code_size } => {
                string(&mut c, u8s, name);
                c.u16(params.len() as u16);
                params.iter().for_each(|p| string(&mut c, u8s, p));
                c.u16(*code_size as u16);
            }
            Op::With { code_size } => c.u16(*code_size as u16),
            Op::StoreRegister(r) => c.u8(*r),
            Op::DefineFunction2(f) => {
                string(&mut c, u8s, &f.name);
                c.u16(f.params.len() as u16);
                c.u8(f.register_count);
                let bits = [f.preload_parent, f.preload_root, f.suppress_super, f.preload_super, f.suppress_arguments, f.preload_arguments, f.suppress_this, f.preload_this];
                c.u8(bits.iter().fold(0, |acc, &b| acc << 1 | b as u8));
                c.u8(f.preload_global as u8);
                for (reg, name) in &f.params {
                    c.u8(*reg);
                    string(&mut c, u8s, name);
                }
                c.u16(f.code_size as u16);
            }
            Op::Try(t) => {
                c.u8((t.catch_in_register as u8) << 2 | (t.finally_flag as u8) << 1 | t.catch_flag as u8);
                c.u16(t.try_size as u16);
                c.u16(t.catch_size as u16);
                c.u16(t.finally_size as u16);
                if t.catch_in_register {
                    c.u8(t.catch_register);
                } else {
                    string(&mut c, u8s, t.catch_name.as_deref().unwrap_or(""));
                }
            }
            Op::StrictMode(m) => c.u8(*m),
            Op::Unknown(d) => c.bytes(d),
        }
        w.u16(c.data.len() as u16);
        w.bytes(&c.data);
    }
}

impl ActionList {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        for a in &self.actions {
            a.write(&mut w, self.charset);
        }
        w.data
    }
}
