//! AVM2 ABC bytecode serialization.

use crate::io::Writer;
use super::{mn_kind, ns_kind, trait_attr, Abc, Multiname, Trait, TraitData};

pub fn write_u30(w: &mut Writer, v: usize) {
    write_u32v(w, (v & 0x3fff_ffff) as u32);
}

pub fn write_u32v(w: &mut Writer, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v > 0 {
            b |= 0x80;
            w.u8(b);
        } else {
            w.u8(b);
            break;
        }
    }
}

/// s32 as its u32 bit pattern.
pub fn write_s32v(w: &mut Writer, v: i32) {
    write_u32v(w, v as u32);
}

fn write_pool<T>(w: &mut Writer, items: &[T], mut write_item: impl FnMut(&mut Writer, &T)) {
    if items.len() <= 1 {
        write_u30(w, 0);
    } else {
        write_u30(w, items.len());
        for item in &items[1..] {
            write_item(w, item);
        }
    }
}

fn write_traits(w: &mut Writer, traits: &[Trait]) {
    write_u30(w, traits.len());
    for t in traits {
        write_u30(w, t.name);
        let tag = (match &t.data {
            TraitData::Slot { .. } => 0,
            TraitData::Method { .. } => 1,
            TraitData::Getter { .. } => 2,
            TraitData::Setter { .. } => 3,
            TraitData::Class { .. } => 4,
            TraitData::Function { .. } => 5,
            TraitData::Const { .. } => 6,
        }) | (t.attrs << 4);
        w.u8(tag);

        match &t.data {
            TraitData::Slot { slot_id, type_name, value } | TraitData::Const { slot_id, type_name, value } => {
                write_u30(w, *slot_id);
                write_u30(w, *type_name);
                if let Some(v) = value {
                    write_u30(w, v.index);
                    w.u8(v.kind);
                } else {
                    write_u30(w, 0);
                }
            }
            TraitData::Method { disp_id, method }
            | TraitData::Getter { disp_id, method }
            | TraitData::Setter { disp_id, method } => {
                write_u30(w, *disp_id);
                write_u30(w, *method);
            }
            TraitData::Class { slot_id, class } => {
                write_u30(w, *slot_id);
                write_u30(w, *class);
            }
            TraitData::Function { slot_id, method } => {
                write_u30(w, *slot_id);
                write_u30(w, *method);
            }
        }

        if t.attrs & trait_attr::METADATA != 0 {
            write_u30(w, t.metadata.len());
            for &m in &t.metadata {
                write_u30(w, m);
            }
        }
    }
}

impl Abc {
    pub fn write(&self, w: &mut Writer) {
        w.u16(self.minor);
        w.u16(self.major);

        // Constant pool
        let c = &self.constants;
        write_pool(w, &c.ints, |w, &i| write_s32v(w, i));
        write_pool(w, &c.uints, |w, &u| write_u32v(w, u));
        write_pool(w, &c.doubles, |w, &d| w.bytes(&d.to_le_bytes()));
        if self.minor >= 17 {
            write_pool(w, &c.decimals, |w, d| w.bytes(d));
        }
        if (self.major, self.minor) >= (47, 16) {
            write_pool(w, &c.floats, |w, &f| w.bytes(&f.to_le_bytes()));
        }
        write_pool(w, &c.strings, |w, s| {
            write_u30(w, s.len());
            w.bytes(s.as_bytes());
        });
        write_pool(w, &c.namespaces, |w, ns| {
            w.u8(ns.kind);
            if ns_kind::ALL.contains(&ns.kind) {
                write_u30(w, ns.name);
            }
        });
        write_pool(w, &c.ns_sets, |w, set| {
            write_u30(w, set.len());
            for &ns in set {
                write_u30(w, ns);
            }
        });
        write_pool(w, &c.multinames, |w, mn| match mn {
            Multiname::QName { attr, ns, name } => {
                w.u8(if *attr { mn_kind::QNAME_A } else { mn_kind::QNAME });
                write_u30(w, *ns);
                write_u30(w, *name);
            }
            Multiname::RtqName { attr, name } => {
                w.u8(if *attr { mn_kind::RTQNAME_A } else { mn_kind::RTQNAME });
                write_u30(w, *name);
            }
            Multiname::RtqNameL { attr } => {
                w.u8(if *attr { mn_kind::RTQNAME_LA } else { mn_kind::RTQNAME_L });
            }
            Multiname::Multiname { attr, name, ns_set } => {
                w.u8(if *attr { mn_kind::MULTINAME_A } else { mn_kind::MULTINAME });
                write_u30(w, *name);
                write_u30(w, *ns_set);
            }
            Multiname::MultinameL { attr, ns_set } => {
                w.u8(if *attr { mn_kind::MULTINAME_LA } else { mn_kind::MULTINAME_L });
                write_u30(w, *ns_set);
            }
            Multiname::TypeName { name, params } => {
                w.u8(mn_kind::TYPENAME);
                write_u30(w, *name);
                write_u30(w, params.len());
                for &p in params {
                    write_u30(w, p);
                }
            }
        });

        // Methods
        write_u30(w, self.methods.len());
        for m in &self.methods {
            write_u30(w, m.param_types.len());
            write_u30(w, m.return_type);
            for &pt in &m.param_types {
                write_u30(w, pt);
            }
            write_u30(w, m.name);
            w.u8(m.flags);
            if m.flags & super::method_flags::HAS_OPTIONAL != 0 {
                write_u30(w, m.optional.len());
                for opt in &m.optional {
                    write_u30(w, opt.index);
                    w.u8(opt.kind);
                }
            }
            if m.flags & super::method_flags::HAS_PARAM_NAMES != 0 {
                for &pn in &m.param_names {
                    write_u30(w, pn);
                }
            }
        }

        // Metadata
        write_u30(w, self.metadata.len());
        for md in &self.metadata {
            write_u30(w, md.name);
            write_u30(w, md.keys.len());
            for &k in &md.keys {
                write_u30(w, k);
            }
            for &v in &md.values {
                write_u30(w, v);
            }
        }

        // Instances and Classes
        write_u30(w, self.instances.len());
        for inst in &self.instances {
            write_u30(w, inst.name);
            write_u30(w, inst.super_name);
            w.u8(inst.flags);
            if inst.flags & super::instance_flags::PROTECTED_NS != 0 {
                write_u30(w, inst.protected_ns);
            }
            write_u30(w, inst.interfaces.len());
            for &iface in &inst.interfaces {
                write_u30(w, iface);
            }
            write_u30(w, inst.iinit);
            write_traits(w, &inst.traits);
        }
        for class in &self.classes {
            write_u30(w, class.cinit);
            write_traits(w, &class.traits);
        }

        // Scripts
        write_u30(w, self.scripts.len());
        for script in &self.scripts {
            write_u30(w, script.init);
            write_traits(w, &script.traits);
        }

        // Method bodies
        let has_exception_names = (self.major, self.minor) > (46, 15);
        write_u30(w, self.bodies.len());
        for b in &self.bodies {
            write_u30(w, b.method);
            write_u30(w, b.max_stack);
            write_u30(w, b.max_regs);
            write_u30(w, b.init_scope_depth);
            write_u30(w, b.max_scope_depth);
            write_u30(w, b.code.len());
            w.bytes(&b.code);
            write_u30(w, b.exceptions.len());
            for ex in &b.exceptions {
                write_u30(w, ex.start);
                write_u30(w, ex.end);
                write_u30(w, ex.target);
                write_u30(w, ex.type_name);
                if has_exception_names {
                    write_u30(w, ex.var_name);
                }
            }
            write_traits(w, &b.traits);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write(&mut w);
        w.data
    }
}
