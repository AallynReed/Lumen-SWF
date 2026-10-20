//! `ActionScript2ClassDetector`: AS2 `class`/`interface` declarations from `__Packages` initializers.

use crate::abc::decomp::item::*;

use super::items::*;
use super::translate::Vars;
use super::uninit::Uninitialized;

type R<T> = Result<T, String>;

fn members_path(item: &It) -> Option<Vec<String>> {
    let mut ret = Vec::new();
    let mut cur = item.clone();
    loop {
        let next = match &cur.borrow().k {
            K::AGetMember { obj, member, .. } => {
                ret.insert(0, direct_string(member)?);
                obj.clone()
            }
            _ => break,
        };
        cur = next;
    }
    match &cur.borrow().k {
        K::AGetVar { name, .. } => ret.insert(0, direct_string(name)?),
        _ => return None,
    }
    Some(ret)
}

fn is_member_of_path(item: &It, path: &[String]) -> Option<String> {
    let p = members_path(item)?;
    if p.len() != path.len() + 1 || p[..path.len()] != *path {
        return None;
    }
    p.last().cloned()
}

fn set_member_to_get_member(item: &It) -> Option<It> {
    match &item.borrow().k {
        K::ASetMember { obj, member, .. } => Some(amk(K::AGetMember { obj: obj.clone(), member: member.clone(), print_obf: false, getter: false }, None, None, prec::PRIMARY, None)),
        K::ASetVar { name, .. } => Some(amk(K::AGetVar { name: name.clone(), print_obf: false, computed: None }, None, None, prec::PRIMARY, None)),
        _ => None,
    }
}

fn new_to_get_member(n: &It) -> R<It> {
    let (src, ls) = (n.borrow().src, n.borrow().line_start);
    match &n.borrow().k {
        K::ANewMethod { obj, name, .. } => Ok(amk(K::AGetMember { obj: obj.clone(), member: name.clone(), print_obf: false, getter: false }, src, ls, prec::PRIMARY, None)),
        K::ANewObject { name, .. } => Ok(amk(K::AGetVar { name: name.clone(), print_obf: false, computed: None }, src, ls, prec::PRIMARY, None)),
        _ => Err("NewMethod or NewObject expected".into()),
    }
}

fn set_members_path(item: &It) -> Option<Vec<String>> {
    match &item.borrow().k {
        K::ASetVar { name, .. } => Some(vec![direct_string(name)?]),
        K::ASetMember { obj, member, .. } => {
            let m = direct_string(member)?;
            let mut p = members_path(obj)?;
            p.push(m);
            Some(p)
        }
        _ => None,
    }
}

fn register_num(item: &It, what: &str) -> R<i32> {
    match &item.borrow().k {
        K::ADirect { v: AVal::Register { num, .. }, .. } => Ok(*num),
        K::ATempReg { reg, .. } => Ok(*reg),
        _ => Err(format!("not a register - {what}")),
    }
}

fn as_string(item: &It, what: &str) -> R<String> {
    if !is_direct(item) {
        return Err(format!("{what} not DirectValue"));
    }
    direct_string(item).ok_or_else(|| format!("{what} not string"))
}

fn strip_global(p: &mut Vec<String>) {
    if p.first().is_some_and(|f| f == "_global") {
        p.remove(0);
    }
}

fn val(it: &It) -> Option<It> {
    it.borrow().value.clone()
}

fn func(it: &It) -> Option<AFunction> {
    match &it.borrow().k {
        K::AFunction(f) => Some((**f).clone()),
        _ => None,
    }
}

fn with_func(it: &It, f: impl FnOnce(&mut AFunction)) {
    if let K::AFunction(x) = &mut it.borrow_mut().k {
        f(x);
    }
}

/// `return undefined` becomes `return` inside constructors and setters.
fn strip_return_undefined(actions: &List) {
    let items: Vec<It> = actions.borrow().clone();
    let fix = |it: &It| {
        if matches!(it.borrow().k, K::AReturn) {
            let undef = val(it).is_some_and(|v| matches!(v.borrow().k, K::ADirect { v: AVal::Undefined, .. }));
            if undef {
                it.borrow_mut().value = None;
            }
        }
    };
    for ti in &items {
        visit_recursively(ti, &mut |i| {
            fix(i);
            true
        });
    }
}

fn remove_range_insert(commands: &List, start: usize, end: usize, item: It) {
    {
        let mut c = commands.borrow_mut();
        for _ in start..=end {
            if start < c.len() {
                c.remove(start);
            }
        }
        c.insert(start, item);
    }
    let next_pop = commands.borrow().get(start + 1).is_some_and(is::pop);
    if next_pop {
        commands.borrow_mut().remove(start + 1);
    }
}

#[allow(clippy::too_many_arguments)]
fn check_class_content(uninit: &Uninitialized, parts: &List, mut parts_pos: usize, cmd_start: usize, cmd_end: usize, commands: &List, class_path: &[String]) -> R<bool> {
    let parts_v: Vec<It> = parts.borrow().clone();
    let n = parts_v.len();
    let mut extends: Option<It> = None;
    let mut implements: Vec<It> = Vec::new();
    let mut instance_reg = -1;
    let mut class_reg = -1;
    let mut constructor: Option<It> = None;
    let mut item: Option<It> = None;
    if n > parts_pos {
        item = Some(parts_v[parts_pos].clone());
        if item.as_ref().is_some_and(|i| matches!(i.borrow().k, K::ATempRegMark { .. })) && parts_pos + 1 < n {
            parts_pos += 1;
            item = Some(parts_v[parts_pos].clone());
        }
        let at = |p: usize| parts_v.get(p).cloned();
        if let Some(it) = item.clone().filter(|i| matches!(i.borrow().k, K::ASetMember { .. })) {
            if let Some(mut mp) = set_members_path(&it) {
                strip_global(&mut mp);
                if mp == class_path {
                    if let Some(v) = val(&it).filter(|v| matches!(v.borrow().k, K::AStoreReg { .. })) {
                        constructor = val(&v);
                        parts_pos += 1;
                        item = at(parts_pos);
                    }
                }
            }
        }
        if let Some(it) = item.clone().filter(|i| matches!(i.borrow().k, K::AExtends { .. })) {
            if let K::AExtends { sup, .. } = &it.borrow().k {
                extends = Some(without_global(sup));
            }
            parts_pos += 1;
            item = at(parts_pos);
        }
        if let Some(it) = item.clone().filter(|i| matches!(i.borrow().k, K::ASetMember { .. })) {
            let mut proto: Vec<String> = class_path.to_vec();
            proto.push("prototype".into());
            if let Some(mut sp) = set_members_path(&it) {
                strip_global(&mut sp);
                if sp == proto && val(&it).is_some_and(|v| matches!(v.borrow().k, K::AStoreReg { .. })) {
                    parts_pos += 1;
                    item = at(parts_pos);
                }
            }
        }
        if let Some(it) = item.clone().filter(|i| matches!(i.borrow().k, K::AStoreReg { .. })) {
            let K::AStoreReg { reg, .. } = it.borrow().k.clone() else { unreachable!() };
            instance_reg = reg;
            let v = val(&it).ok_or("Not Getmember in StoreRegister")?;
            let (gobj, gmember) = match &v.borrow().k {
                K::AGetMember { obj, member, .. } => (obj.clone(), member.clone()),
                _ => return Err("Not Getmember in StoreRegister".into()),
            };
            let treg = match &gobj.borrow().k {
                K::ATempReg { reg, .. } => *reg,
                _ => return Err("Getmember does not have TemporaryRegister as object".into()),
            };
            class_reg = treg;
            if as_string(&gmember, "memberName")? != "prototype" {
                return Err("memberName not \"prototype\"".into());
            }
            let tv = val(&gobj).unwrap();
            if !matches!(tv.borrow().k, K::ASetMember { .. } | K::ASetVar { .. }) {
                return Err("temporaryreg value not setmember/setvariable".into());
            }
            let mut path = set_members_path(&tv).filter(|p| !p.is_empty()).ok_or("Cannot detect class - tempreg value is not a path")?;
            strip_global(&mut path);
            if path != class_path {
                return Err("temporaryreg value does not match class path".into());
            }
            let sv = val(&tv).filter(|v| matches!(v.borrow().k, K::AStoreReg { .. })).ok_or("Constructor expected to be in storeregister")?;
            let f = val(&sv).filter(|v| matches!(v.borrow().k, K::AFunction(_))).ok_or("Constructor expected as functionitem")?;
            constructor = Some(f);
            parts_pos += 1;
        }
    }
    let mut class_name = amk(K::AGetVar { name: dstr(&class_path[0]), print_obf: false, computed: None }, None, None, prec::PRIMARY, None);
    for p in &class_path[1..] {
        class_name = amk(K::AGetMember { obj: class_name, member: dstr(p), print_obf: false, getter: false }, None, None, prec::PRIMARY, None);
    }
    let mut traits: Vec<(It, Option<It>)> = Vec::new();
    let mut statics: Vec<bool> = Vec::new();
    while parts_pos < n {
        let mut it = parts_v[parts_pos].clone();
        if is::push(&it) {
            let v = it.borrow().val();
            it = v;
        }
        let k = it.borrow().k.clone();
        match &k {
            K::ASetMember { obj, member, .. } => {
                let (cur_reg, reg_value) = match &obj.borrow().k {
                    K::ATempReg { reg, .. } => (*reg, val(obj)),
                    K::ADirect { v: AVal::Register { num, .. }, computed } => (*num, computed.clone()),
                    _ => (-1, None),
                };
                if cur_reg == -1 && !matches!(obj.borrow().k, K::ATempReg { .. } | K::ADirect { v: AVal::Register { .. }, .. }) {
                    let mut path = set_members_path(&it).filter(|p| !p.is_empty()).ok_or("invalid setmember")?;
                    strip_global(&mut path);
                    if path != class_path {
                        return Err("wrong path in setmember".into());
                    }
                    let iface = without_global(&set_member_to_get_member(&it).unwrap());
                    let f = val(&it).and_then(|v| func(&v)).ok_or("not a function in setmember")?;
                    if !f.name.is_empty() || !f.actions.borrow().is_empty() || !f.params.is_empty() {
                        return Err("not an empty unnamed function in setmember".into());
                    }
                    parts_pos += 1;
                    while parts_pos < n {
                        let x = parts_v[parts_pos].clone();
                        match &x.borrow().k {
                            K::AImplements { supers, .. } => {
                                if !implements.is_empty() {
                                    return Err("multiple implementsAction".into());
                                }
                                implements = supers.borrow().clone();
                            }
                            _ => return Err("unknown iface item".into()),
                        }
                        parts_pos += 1;
                    }
                    let ifs = amk(K::AInterface { name: iface, supers: new_list(implements) }, None, None, prec::NOPRECEDENCE, None);
                    remove_range_insert(commands, cmd_start, cmd_end, ifs);
                    return Ok(true);
                }
                if cur_reg != instance_reg && cur_reg != class_reg {
                    let sm2 = reg_value.filter(|r| matches!(r.borrow().k, K::ASetMember { .. })).ok_or("temp register do not contain setmember")?;
                    let K::ASetMember { obj: o2, member: m2, .. } = sm2.borrow().k.clone() else { unreachable!() };
                    let (path_source, is_proto) = if as_string(&m2, "objectName")? == "prototype" { (o2.clone(), true) } else { (set_member_to_get_member(&sm2).unwrap(), false) };
                    let mut mp = members_path(&path_source).ok_or("Invalid pathsource")?;
                    strip_global(&mut mp);
                    if mp != class_path {
                        return Err("Invalid path of setmember".into());
                    }
                    let sr = val(&sm2).filter(|v| matches!(v.borrow().k, K::AStoreReg { .. })).ok_or("Not storeregister")?;
                    if !matches!(&sr.borrow().k, K::AStoreReg { reg, .. } if *reg == cur_reg) {
                        return Err("Invalid storeregister".into());
                    }
                    let srv = val(&sr).unwrap();
                    if is_proto && matches!(srv.borrow().k, K::ANewMethod { .. } | K::ANewObject { .. }) {
                        extends = Some(new_to_get_member(&srv)?);
                        instance_reg = cur_reg;
                    } else if !is_proto && matches!(srv.borrow().k, K::AFunction(_)) {
                        constructor = Some(srv);
                        class_reg = cur_reg;
                    } else {
                        return Err("invalid storeregister value".into());
                    }
                }
                let v = val(&it);
                if let Some(v) = &v {
                    with_func(v, |f| f.calculated_name = Some(member.clone()));
                }
                traits.push((member.clone(), v));
                if cur_reg == instance_reg {
                    statics.push(false);
                } else if cur_reg == class_reg {
                    statics.push(true);
                }
            }
            K::AImplements { supers, .. } => implements = supers.borrow().clone(),
            K::ACallMethod { obj, name, args } => {
                let m = as_string(name, "push methodName")?;
                if m != "addProperty" {
                    return Err(format!("unknown push method name: {m}"));
                }
                let robj = register_num(obj, "addProperty not on register")?;
                if robj != instance_reg && robj != class_reg {
                    return Err("unexpected addProperty object register".into());
                }
                let a: Vec<It> = args.borrow().clone();
                if a.len() != 3 {
                    return Err("invalid number of arguments to addProperty".into());
                }
                let pname = as_string(&a[0], "propertyName")?;
                let empty_fn = |f: &AFunction| f.actions.borrow().is_empty() && f.name.is_empty() && f.params.is_empty();
                let getter_k = a[1].borrow().k.clone();
                match &getter_k {
                    K::AGetMember { obj: go, member: gm, .. } => {
                        if register_num(go, "getter member not register")? != robj {
                            return Err("getter register does not match property register".into());
                        }
                        let gname = as_string(gm, "getter memberName")?;
                        if gname != format!("__get__{pname}") {
                            parts_pos += 1;
                            continue;
                        }
                        for (k, v) in &traits {
                            if direct_string(k).as_deref() == Some(&gname) {
                                if let Some(v) = v {
                                    with_func(v, |f| f.getter = true);
                                }
                            }
                        }
                    }
                    K::AFunction(f) if empty_fn(f) => {}
                    _ => return Err(format!("unexpected getter value for property {pname}")),
                }
                let setter_k = a[2].borrow().k.clone();
                match &setter_k {
                    K::AGetMember { obj: so, member: sm, .. } => {
                        if register_num(so, "setter member")? != robj {
                            return Err("setter register does not match property register".into());
                        }
                        let sname = as_string(sm, "setter memberNAme")?;
                        if sname != format!("__set__{pname}") {
                            parts_pos += 1;
                            continue;
                        }
                        for (k, v) in &traits {
                            if direct_string(k).as_deref() == Some(&sname) {
                                let Some(v) = v else { continue };
                                let Some(f) = func(v) else { continue };
                                with_func(v, |f| f.setter = true);
                                strip_return_undefined(&f.actions);
                                let acts = f.actions.clone();
                                let len = acts.borrow().len();
                                if len > 0 {
                                    let mut pos = len as i64 - 1;
                                    if is::script_end(&acts.borrow()[pos as usize]) {
                                        pos -= 1;
                                    }
                                    if pos >= 0 {
                                        let last = acts.borrow()[pos as usize].clone();
                                        let getter_ret = matches!(last.borrow().k, K::AReturn) && val(&last).is_some_and(|v| matches!(v.borrow().k, K::AGetMember { getter: true, .. }));
                                        if getter_ret {
                                            acts.borrow_mut().remove(pos as usize);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    K::AFunction(f) if empty_fn(f) => {}
                    _ => return Err(format!("unexpected setter value for property {pname}")),
                }
            }
            K::ACallFunction { name, args, .. } => {
                let f = as_string(name, "pushitem function name")?;
                if f != "ASSetPropFlags" {
                    return Err(format!("unknown pushitem function call {f}"));
                }
                let a: Vec<It> = args.borrow().clone();
                if a.len() != 3 {
                    return Err("Invalid number of arguments to ASSetPropFlags".into());
                }
                match &a[0].borrow().k {
                    K::ADirect { v: AVal::Register { num, .. }, .. } => {
                        if *num != instance_reg {
                            return Err("ASSetPropFlags not on instanceReg".into());
                        }
                    }
                    _ => {
                        let mut p = members_path(&a[0]);
                        if let Some(p) = &mut p {
                            strip_global(p);
                        }
                        let mut proto: Vec<String> = class_path.to_vec();
                        proto.push("prototype".into());
                        if p.as_ref() != Some(&proto) {
                            return Err("ASSetPropFlags not on prototype".into());
                        }
                    }
                }
                if !matches!(a[1].borrow().k, K::ADirect { v: AVal::Null, .. }) {
                    return Err("ASSetPropFlags properties param not null".into());
                }
                if !matches!(a[2].borrow().k, K::ADirect { v: AVal::Int(1), .. }) {
                    return Err("ASSetPropFlags flags not set to 1".into());
                }
            }
            K::ADirect { .. } | K::ScriptEnd | K::ATempRegMark { .. } => {}
            _ => return Err("unknown item".into()),
        }
        parts_pos += 1;
    }
    if let Some(c) = &constructor {
        statics.insert(0, false);
        let base = dstr(class_path.last().unwrap());
        with_func(c, |f| f.calculated_name = Some(base.clone()));
        traits.insert(0, (base, Some(c.clone())));
        if let Some(f) = func(c) {
            strip_return_undefined(&f.actions);
        }
    }
    let full = members_path(&class_name).unwrap_or_default().join(".");
    if let Some(list) = uninit.get(&full) {
        for (t, (n, is_static)) in list.iter().enumerate() {
            statics.insert(t, *is_static);
            traits.insert(t, (dstr(n), None));
        }
    }
    let cls = class_item(class_name, extends, implements, traits, statics);
    remove_range_insert(commands, cmd_start, cmd_end, cls);
    Ok(true)
}

/// `ClassActionItem`. Its own `this.x` scan only matches compiler items, so a decompiled class
/// gets its extra fields from the uninitialized-fields detector instead.
fn class_item(name: It, extends: Option<It>, implements: Vec<It>, traits: Vec<(It, Option<It>)>, statics: Vec<bool>) -> It {
    amk(
        K::AClass(Box::new(AClass { name, extends, implements: new_list(implements), traits, statics, uninitialized: vec![] })),
        None,
        None,
        prec::NOPRECEDENCE,
        None,
    )
}

fn expand_ternars(commands: &List) {
    let n = commands.borrow().len();
    for i in 0..n {
        let c = commands.borrow()[i].clone();
        let K::Ternar { expr, on_true, on_false } = c.borrow().k.clone() else { continue };
        let side = |x: &It| match &x.borrow().k {
            K::Comma { cmds } => cmds.clone(),
            _ => new_list(vec![x.clone()]),
        };
        let ifi = if_item(None, None, expr, side(&on_true), side(&on_false));
        ifi.borrow_mut().as2 = true;
        commands.borrow_mut()[i] = ifi;
    }
}

fn if_parts(it: &It) -> Option<(It, List, List)> {
    match &it.borrow().k {
        K::If { expr, on_true, on_false, .. } => Some((expr.clone(), on_true.clone(), on_false.clone())),
        _ => None,
    }
}

fn not_value(e: &It) -> Option<It> {
    if is::not(e) {
        return val(e);
    }
    None
}

fn check_if_variants(uninit: &Uninitialized, commands: &List, pos: usize) -> bool {
    expand_ternars(commands);
    let mut path1: Vec<String> = vec!["_global".into()];
    let mut check_pos = pos;
    while check_pos < commands.borrow().len() {
        let t = commands.borrow()[check_pos].clone();
        let Some((expr, on_true, _)) = if_parts(&t) else { break };
        let Some(cond) = not_value(&expr) else { break };
        let Some(m) = is_member_of_path(&cond, &path1) else { break };
        path1.push(m);
        let single_new = on_true.borrow().len() == 1
            && matches!(on_true.borrow()[0].borrow().k, K::ASetMember { .. })
            && val(&on_true.borrow()[0]).is_some_and(|v| matches!(v.borrow().k, K::ANewObject { .. }));
        if single_new {
            if check_pos + 1 < commands.borrow().len() && is::pop(&commands.borrow()[check_pos + 1]) {
                check_pos += 1;
            }
            check_pos += 1;
            continue;
        }
        let class_path: Vec<String> = path1[1..].to_vec();
        let r = if on_true.borrow().is_empty() {
            let end = commands.borrow().len() - 1;
            check_class_content(uninit, commands, check_pos + 1, pos, end, commands, &class_path)
        } else {
            check_class_content(uninit, &on_true, 0, pos, check_pos, commands, &class_path)
        };
        if r.unwrap_or(false) {
            return true;
        }
        break;
    }
    let first = commands.borrow().get(pos).cloned();
    if let Some((expr, on_true, _)) = first.as_ref().and_then(if_parts) {
        if let Some(mp) = not_value(&expr).and_then(|v| members_path(&v)) {
            if on_true.borrow().len() >= mp.len() {
                let mut cur: Vec<String> = Vec::new();
                let mut cp = 0;
                let mut ok = true;
                while cp + 1 < mp.len() {
                    cur.push(mp[cp].clone());
                    let p = on_true.borrow()[cp].clone();
                    let Some((e2, _, _)) = if_parts(&p) else {
                        ok = false;
                        break;
                    };
                    if let Some(v2) = not_value(&e2) {
                        if members_path(&v2).as_ref() != Some(&cur) {
                            ok = false;
                            break;
                        }
                    }
                    cp += 1;
                }
                if ok && check_class_content(uninit, &on_true, cp, pos, pos, commands, &mp).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

/// `ActionScript2ClassDetector.checkClass`.
pub fn check_class(commands: &List, _vars: &Vars, uninit: &Uninitialized) {
    let local = copy_list(commands);
    let mut changed = false;
    let mut pos = 0;
    while pos < local.borrow().len() {
        if check_if_variants(uninit, &local, pos) {
            changed = true;
        }
        pos += 1;
    }
    if changed {
        let v: Vec<It> = local.borrow().clone();
        *commands.borrow_mut() = v;
    }
}
