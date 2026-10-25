//! AS3 source replacement through the mxmlc port: the edited script pack is compiled as mxmlc compiles one source
//! among the SWF's ABCs, then linked into the selected `DoABC` the way `PostLink` merges unit ABCs.
//!
//! The ABC is cut into the scripts before the replaced one, the new unit and the scripts after it, and those blocks go
//! through `macromedia.abc`'s encoder in that order, so the new script keeps the replaced script's index and the entry
//! script stays last. That merge drops debug information and re-encodes every body, so it is used only when the
//! existing scripts carry none, as `PostLink` leaves them; anything else gets the unit appended without touching the
//! other scripts.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::abc::opcodes::{decode, Dialect, Instruction, Operand};
use crate::abc::{method_flags, ns_kind, Abc, Exception, Multiname, Namespace, Script, Trait, TraitData, ValueKind};
use crate::action::asm::ParseError;
use crate::asc::flex2::compiler::abc_library;
use crate::asc::flex2::compiler::source::canonical_path;
use crate::asc::flex2::linker::DoAbc;
use crate::asc::flex2::tools::post_link;
use crate::asc::flex2::{compile_unit, ApplicationOptions, Diagnostic, Level, UnitOptions};

fn fail(message: impl Into<String>) -> ParseError {
    ParseError { line: 0, message: message.into() }
}

fn is_public(abc: &Abc, t: &Trait) -> bool {
    matches!(abc.ns_kind_of(t.name), ns_kind::PACKAGE | ns_kind::PACKAGE_INTERNAL)
}

/// Compiles `text` in place of a script pack of `abcs[selected]` and returns that `DoABC`'s new ABC.
/// `renames` pairs a name as the decompiler printed it with the name in the ABC.
pub fn replace_script(abcs: &[Abc], selected: usize, script: usize, trait_index: Option<usize>, text: &str, renames: &[(String, String)]) -> Result<Vec<u8>, ParseError> {
    let abc = abcs.get(selected).ok_or_else(|| fail("not an ABC tag"))?;
    let s = abc.scripts.get(script).ok_or_else(|| fail("script not found"))?;
    let publics = s.traits.iter().filter(|t| is_public(abc, t)).count();
    // A whole script is replaced like a script's only definition.
    let simple = publics == 1 || trait_index.is_none();
    let pack: Vec<usize> = match trait_index {
        Some(t) if t < s.traits.len() => crate::abc::scripts::pack_traits(abc, script, t),
        Some(_) => return Err(fail("script not found")),
        None => (0..s.traits.len()).collect(),
    };
    if !simple && !trait_index.is_some_and(|t| matches!(s.traits[t].data, TraitData::Class { .. })) {
        return Err(fail("Only a class can be compiled on its own in a script that defines several things"));
    }
    let named = trait_index.filter(|&t| is_public(abc, &s.traits[t])).or_else(|| s.traits.iter().position(|t| is_public(abc, t)));
    let mut names = Renames::default();
    let source = rename_source(text, renames, &mut names);
    let (package, name) = match named.map(|t| &abc.constants.multinames[s.traits[t].name]) {
        Some(Multiname::QName { ns, name, .. }) => (abc.str(abc.constants.namespaces[*ns].name).to_string(), abc.str(*name).to_string()),
        _ => (String::new(), format!("script_{script}")),
    };
    let (package, name) = (names.to_placeholder(&package).unwrap_or(package), names.to_placeholder(&name).unwrap_or(name));

    let root = std::env::temp_dir().join("lumen-as3");
    let mut file = root.clone();
    let mut shown = PathBuf::new();
    for p in package.split('.').filter(|p| !p.is_empty()) {
        file.push(p);
        shown.push(p);
    }
    file.push(format!("{name}.as"));
    shown.push(format!("{name}.as"));
    let plan = Plan::new(abc, script, &pack, simple)?;
    let opts = UnitOptions {
        source,
        path: file.clone(),
        root,
        abcs: context(abcs, selected, plan.script_blocks(), &names),
        same_build: true,
        app: ApplicationOptions { playerglobal: true, omit_trace_statements: !keeps_traces(abc), ..Default::default() },
        ..Default::default()
    };
    let unit = compile_unit(&opts).map_err(|d| report(&d, &canonical_path(&file), &names.to_written(&shown.to_string_lossy()), &names))?;

    let mut bytes = unit.abc;
    if !names.names.is_empty() {
        let mut a = Abc::parse(&bytes).map_err(|e| fail(e.to_string()))?;
        for s in a.constants.strings.iter_mut() {
            if s.contains(MARK) {
                *s = names.to_raw(s);
            }
        }
        bytes = a.to_bytes();
    }
    let unit = optimize(bytes, &metadata_names(abc))?;
    plan.link(script, &unit)
}

/// Whether the ABC was built with trace calls kept: a `-debug` build, or code that still names `trace`.
fn keeps_traces(abc: &Abc) -> bool {
    let named_trace = abc.constants.multinames.iter().any(|m| match m {
        Multiname::QName { name, .. } | Multiname::Multiname { name, .. } => abc.constants.strings.get(*name).is_some_and(|s| s == "trace"),
        _ => false,
    });
    let dialect = Dialect::of(abc.major, abc.minor);
    named_trace || abc.bodies.iter().any(|b| decode(&b.code, dialect).is_ok_and(|code| code.iter().any(|i| i.code == 0xf0)))
}

/// `PostLink`'s merge of the unit on its own: debug information and unkept metadata dropped, peephole optimizer on.
/// The player's verifier check follows, as the old compiler made it.
fn optimize(abc: Vec<u8>, keep: &[String]) -> Result<Abc, ParseError> {
    let mut one = vec![DoAbc { name: String::new(), flag: 1, abc }];
    post_link::merge(&mut one, false, true, "unit", keep);
    if one.len() != 1 || one[0].name != "unit" {
        return Err(fail("mxmlc's optimizer could not encode the compiled code"));
    }
    let mut unit = Abc::parse(&one[0].abc).map_err(|e| fail(e.to_string()))?;
    for b in 0..unit.bodies.len() {
        crate::abc::stats::fit_body(&mut unit, b).map_err(|(_, what)| fail(format!("the compiled code would not pass the player's verifier ({what})")))?;
    }
    Ok(unit)
}

fn metadata_names(abc: &Abc) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in &abc.metadata {
        let n = abc.str(m.name);
        if !out.iter().any(|o| o == n) {
            out.push(n.to_string());
        }
    }
    out
}

/// The ABCs the source compiles against: the selected one's other scripts, then the SWF's other ABCs.
/// A definition already given by `playerglobal` or an earlier block is dropped, as the player would never use it, and
/// so that `SwcGroup` does not discard the whole block for it.
fn context(abcs: &[Abc], selected: usize, sel: Vec<Abc>, names: &Renames) -> Vec<Vec<u8>> {
    let mut seen: HashSet<String> = abc_library::playerglobal_definitions().map(String::from).collect();
    let mut out = Vec::new();
    let others = abcs.iter().enumerate().filter(|(i, _)| *i != selected).map(|(_, a)| a.clone());
    for mut abc in sel.into_iter().chain(others) {
        if !names.names.is_empty() {
            for s in abc.constants.strings.iter_mut() {
                if let Some(p) = names.to_placeholder(s) {
                    *s = p;
                }
            }
        }
        let (defs, _) = abc_library::describe(&abc);
        let dup: Vec<String> = defs.iter().filter(|d| seen.contains(*d)).cloned().collect();
        if !dup.is_empty() {
            abc = abc_library::strip_definitions(&abc, &dup);
        }
        seen.extend(defs);
        out.push(abc.to_bytes());
    }
    out
}

/// mxmlc's errors as one `ParseError` on the first error's line.
fn report(diagnostics: &[Diagnostic], path: &str, shown: &str, names: &Renames) -> ParseError {
    let errors: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.level == Level::Error).collect();
    let line = errors.iter().find(|d| d.line > 0).map_or(0, |d| d.line as usize);
    let message = errors.iter().map(|d| names.to_written(&d.text.replace(path, shown))).collect::<Vec<_>>().join("\n\n");
    ParseError { line, message: if message.is_empty() { "the source did not compile".into() } else { message } }
}

const MARK: &str = "$lumen$";

/// Names the source spells differently from the ABC, compiled under placeholders that are valid identifiers.
#[derive(Default)]
struct Renames {
    /// Placeholder, name in the ABC, text in the source.
    names: Vec<(String, String, String)>,
}

impl Renames {
    fn placeholder(&mut self, raw: &str, written: &str) -> String {
        if let Some((p, _, _)) = self.names.iter().find(|(_, r, _)| r == raw) {
            return p.clone();
        }
        let p = format!("{MARK}{}$", self.names.len());
        self.names.push((p.clone(), raw.to_string(), written.to_string()));
        p
    }

    fn to_raw(&self, s: &str) -> String {
        self.names.iter().rev().fold(s.to_string(), |s, (p, raw, _)| s.replace(p, raw))
    }

    fn to_written(&self, s: &str) -> String {
        self.names.iter().rev().fold(s.to_string(), |s, (p, _, w)| s.replace(p, w))
    }

    /// `s` with each name, whole or as a `.`/`:`-separated part, swapped for its placeholder.
    fn to_placeholder(&self, s: &str) -> Option<String> {
        let find = |part: &str| self.names.iter().find(|(_, r, _)| r == part).map(|(p, _, _)| p.clone());
        if self.names.is_empty() {
            return None;
        }
        if let Some(p) = find(s) {
            return Some(p);
        }
        let mut out = String::new();
        let mut changed = false;
        for part in s.split_inclusive(['.', ':']) {
            let (body, sep) = match part.strip_suffix(['.', ':']) {
                Some(b) => (b, &part[b.len()..]),
                None => (part, ""),
            };
            match find(body) {
                Some(p) => {
                    out.push_str(&p);
                    changed = true;
                }
                None => out.push_str(body),
            }
            out.push_str(sep);
        }
        changed.then_some(out)
    }
}

fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// The source with `§...§` identifiers and renamed identifiers swapped for placeholders, strings and comments kept.
fn rename_source(text: &str, renames: &[(String, String)], names: &mut Renames) -> String {
    let cs: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let next = cs.get(i + 1).copied();
        let start = i;
        if c == '/' && next == Some('/') {
            while i < cs.len() && !matches!(cs[i], '\n' | '\r') {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < cs.len() && !(cs[i] == '*' && cs.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(cs.len());
        } else if c == '"' || c == '\'' {
            i += 1;
            while i < cs.len() && cs[i] != c && !matches!(cs[i], '\n' | '\r') {
                i += if cs[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(cs.len());
        } else if c == '\u{a7}' && next != Some('\u{a7}') {
            if let Some((raw, end)) = section_identifier(&cs, i) {
                let written: String = cs[i..end].iter().collect();
                out.push_str(&names.placeholder(&raw, &written));
                i = end;
                continue;
            }
            i += 1;
        } else if c == '\u{a7}' {
            i += 2;
        } else if ident_start(c) {
            while i < cs.len() && ident_part(cs[i]) {
                i += 1;
            }
            let w: String = cs[start..i].iter().collect();
            match renames.iter().find(|(p, _)| *p == w) {
                Some((_, raw)) => out.push_str(&names.placeholder(raw, &w)),
                None => out.push_str(&w),
            }
            continue;
        } else {
            i += 1;
        }
        out.extend(&cs[start..i]);
    }
    out
}

/// The name inside `§...§` at `start` and the position after it, undoing `escapeOIdentifier`.
fn section_identifier(cs: &[char], start: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut repeat = 1;
    let mut k = start + 1;
    loop {
        let c = *cs.get(k)?;
        let (v, n) = match c {
            '\u{a7}' => return Some((out, k + 1)),
            '\r' | '\n' => return None,
            '\\' => match *cs.get(k + 1)? {
                'n' => ('\n', 2),
                'r' => ('\r', 2),
                'b' => ('\u{8}', 2),
                't' => ('\t', 2),
                'f' => ('\u{c}', 2),
                'x' => {
                    let h: String = cs.get(k + 2..k + 4)?.iter().collect();
                    (char::from_u32(u32::from_str_radix(&h, 16).ok()?)?, 4)
                }
                '{' => {
                    let end = (k + 2..cs.len()).find(|&j| cs[j] == '}')?;
                    repeat = cs[k + 2..end].iter().collect::<String>().parse().ok()?;
                    k = end + 1;
                    continue;
                }
                e => (e, 2),
            },
            c => (c, 1),
        };
        out.extend(std::iter::repeat_n(v, repeat));
        repeat = 1;
        k += n;
    }
}

/// The selected ABC with the replaced pack taken out of its script, and what of it the other scripts still use.
struct Plan {
    base: Abc,
    /// The scripts that stay, in order; the unit goes in before the one at the replaced index.
    kept: Vec<usize>,
    code: Vec<Option<Vec<Instruction>>>,
    /// The methods and classes to keep, or None when some body cannot be decoded and nothing can be renumbered.
    keep: Option<(Vec<bool>, Vec<bool>)>,
}

impl Plan {
    fn new(abc: &Abc, script: usize, pack: &[usize], simple: bool) -> Result<Plan, ParseError> {
        let dialect = Dialect::of(abc.major, abc.minor);
        let mut base = abc.clone();
        if !simple {
            let init = base.scripts[script].init;
            let bi = base.body_of.get(init).copied().flatten().ok_or_else(|| fail("The script has no initializer"))?;
            for &t in pack {
                if let TraitData::Class { class, .. } = abc.scripts[script].traits[t].data {
                    strip_class_init(&mut base.bodies[bi], class, dialect).ok_or_else(|| fail("The script initializer does not set up this class in a way that can be replaced"))?;
                }
            }
            let mut i = 0;
            base.scripts[script].traits.retain(|_| {
                i += 1;
                !pack.contains(&(i - 1))
            });
        }
        let kept: Vec<usize> = (0..base.scripts.len()).filter(|&s| !simple || s != script).collect();
        let code: Vec<Option<Vec<Instruction>>> = base.bodies.iter().map(|b| decode(&b.code, dialect).ok()).collect();
        let reach_kept = Reach::of(&base, &code, kept.iter().map(|&s| &base.scripts[s]), &[]);
        let replaced: Vec<&Trait> = if simple { Vec::new() } else { pack.iter().map(|&t| &abc.scripts[script].traits[t]).collect() };
        let reach_old = Reach::of(&base, &code, simple.then(|| &abc.scripts[script]), &replaced);
        let keep = match (reach_kept, reach_old) {
            (Some((km, kc)), Some((om, oc))) => Some((km.iter().zip(&om).map(|(k, o)| *k || !*o).collect(), kc.iter().zip(&oc).map(|(k, o)| *k || !*o).collect())),
            _ => None,
        };
        Ok(Plan { base, kept, code, keep })
    }

    /// The ABC without the replaced pack. When nothing can be renumbered the old definitions stay behind unreferenced.
    fn kept_abc(&self) -> Abc {
        match &self.keep {
            Some((m, c)) => subset(&self.base, &self.kept, m, c, Dialect::of(self.base.major, self.base.minor)),
            None => {
                let mut a = self.base.clone();
                a.scripts = self.kept.iter().map(|&s| self.base.scripts[s].clone()).collect();
                a
            }
        }
    }

    /// The kept scripts one block each, as the units of the build they came from: one block for them all would be a
    /// unit that both extends and is extended by a class of the source. Each goes through the encoder to keep only
    /// the constants it uses.
    fn script_blocks(&self) -> Vec<Abc> {
        let dialect = Dialect::of(self.base.major, self.base.minor);
        let mut out = Vec::new();
        for &s in &self.kept {
            let Some((m, c)) = self.keep.as_ref().and_then(|_| Reach::of(&self.base, &self.code, std::iter::once(&self.base.scripts[s]), &[])) else {
                return vec![self.kept_abc()];
            };
            let block = subset(&self.base, &[s], &m, &c, dialect);
            let keep = metadata_names(&block);
            let mut one = vec![DoAbc { name: String::new(), flag: 1, abc: block.to_bytes() }];
            post_link::merge(&mut one, false, false, "block", &keep);
            out.push(if one[0].name == "block" { Abc::parse(&one[0].abc).unwrap_or(block) } else { block });
        }
        out
    }

    /// Puts the compiled unit in place of the replaced pack, at script index `at`.
    fn link(&self, at: usize, unit: &Abc) -> Result<Vec<u8>, ParseError> {
        if let Some((keep_m, keep_c)) = &self.keep {
            if release_form(&self.base, &self.code, keep_m) {
                if let Some(bytes) = encoder_link(&self.base, &self.code, &self.kept, at, keep_m, keep_c, unit) {
                    return Ok(bytes);
                }
            }
        }
        append_link(self.kept_abc(), at, unit)
    }
}

/// Whether the kept code is as `PostLink` leaves a release build: no method or parameter names, no debug opcodes.
fn release_form(abc: &Abc, code: &[Option<Vec<Instruction>>], keep_m: &[bool]) -> bool {
    let methods = abc.methods.iter().zip(keep_m).filter(|(_, k)| **k).all(|(m, _)| m.name == 0 && m.flags & method_flags::HAS_PARAM_NAMES == 0);
    let bodies = abc.bodies.iter().zip(code).filter(|(b, _)| keep_m.get(b.method).copied().unwrap_or(false)).all(|(_, c)| c.as_ref().is_some_and(|c| c.iter().all(|i| !matches!(i.code, 0xef..=0xf1))));
    methods && bodies
}

/// The `PostLink` merge of [scripts before, unit, scripts after], or of [kept scripts, unit] with the unit's scripts
/// moved into place when the two sides share code or a private namespace. Debug info is dropped (there is none),
/// metadata kept, and the peephole pass skipped so the other scripts are re-encoded unchanged; the unit had it alone.
fn encoder_link(base: &Abc, code: &[Option<Vec<Instruction>>], kept: &[usize], at: usize, keep_m: &[bool], keep_c: &[bool], unit: &Abc) -> Option<Vec<u8>> {
    let dialect = Dialect::of(base.major, base.minor);
    let (before, after) = kept.split_at(at);
    let (bm, bc) = Reach::of(base, code, before.iter().map(|&s| &base.scripts[s]), &[])?;
    let (am, ac) = Reach::of(base, code, after.iter().map(|&s| &base.scripts[s]), &[])?;
    let split = |b: &[bool], a: &[bool], k: &[bool]| b.iter().zip(a).zip(k).all(|((b, a), k)| !(*b && *a) && (*b || *a || !*k));
    let three = split(&bm, &am, keep_m)
        && split(&bc, &ac, keep_c)
        && private_namespaces(base, code, before, &bm, &bc).is_disjoint(&private_namespaces(base, code, after, &am, &ac));
    let mut pieces = Vec::new();
    if three {
        if !before.is_empty() {
            pieces.push(subset(base, before, &bm, &bc, dialect).to_bytes());
        }
        pieces.push(unit.to_bytes());
        if !after.is_empty() {
            pieces.push(subset(base, after, &am, &ac, dialect).to_bytes());
        }
    } else {
        pieces.push(subset(base, kept, keep_m, keep_c, dialect).to_bytes());
        pieces.push(unit.to_bytes());
    }
    let mut keep = metadata_names(base);
    for n in metadata_names(unit) {
        if !keep.contains(&n) {
            keep.push(n);
        }
    }
    let mut blocks: Vec<DoAbc> = pieces.into_iter().map(|abc| DoAbc { name: String::new(), flag: 1, abc }).collect();
    post_link::merge(&mut blocks, false, false, "merged", &keep);
    if blocks.len() != 1 || blocks[0].name != "merged" {
        return None;
    }
    let mut bytes = blocks.pop()?.abc;
    if !three {
        let mut merged = Abc::parse(&bytes).ok()?;
        let tail = merged.scripts.split_off(merged.scripts.len() - unit.scripts.len());
        merged.scripts.splice(at..at, tail);
        bytes = merged.to_bytes();
    }
    // The encoder writes the last block's version.
    bytes[..2].copy_from_slice(&base.minor.to_le_bytes());
    bytes[2..4].copy_from_slice(&base.major.to_le_bytes());
    Some(bytes)
}

/// The unit appended to `kept` with its constants pooled in, its scripts moved to index `at`.
fn append_link(mut kept: Abc, at: usize, unit: &Abc) -> Result<Vec<u8>, ParseError> {
    let first = kept.scripts.len();
    append(&mut kept, unit)?;
    let tail = kept.scripts.split_off(first);
    kept.scripts.splice(at..at, tail);
    Ok(kept.to_bytes())
}

enum Item {
    Method(usize),
    Class(usize),
}

/// The methods and classes a set of scripts and traits can create or call.
struct Reach<'a> {
    abc: &'a Abc,
    code: &'a [Option<Vec<Instruction>>],
    methods: Vec<bool>,
    classes: Vec<bool>,
    todo: Vec<Item>,
}

impl<'a> Reach<'a> {
    /// None when a reached body cannot be decoded.
    fn of<'s>(abc: &'a Abc, code: &'a [Option<Vec<Instruction>>], scripts: impl IntoIterator<Item = &'s Script>, traits: &[&Trait]) -> Option<(Vec<bool>, Vec<bool>)> {
        let mut r = Reach { abc, code, methods: vec![false; abc.methods.len()], classes: vec![false; abc.instances.len()], todo: Vec::new() };
        for s in scripts {
            r.todo.push(Item::Method(s.init));
            r.traits(&s.traits);
        }
        for t in traits {
            r.traits(std::slice::from_ref(*t));
        }
        r.run()
    }

    fn traits(&mut self, ts: &[Trait]) {
        for t in ts {
            match t.data {
                TraitData::Class { class, .. } => self.todo.push(Item::Class(class)),
                _ => self.todo.extend(t.method().map(Item::Method)),
            }
        }
    }

    fn run(mut self) -> Option<(Vec<bool>, Vec<bool>)> {
        let abc = self.abc;
        while let Some(item) = self.todo.pop() {
            match item {
                Item::Method(m) => {
                    if m >= self.methods.len() || self.methods[m] {
                        continue;
                    }
                    self.methods[m] = true;
                    let Some(bi) = abc.body_of.get(m).copied().flatten() else { continue };
                    for i in self.code[bi].as_ref()? {
                        for (k, &v) in i.kinds.iter().zip(&i.operands) {
                            match k {
                                Operand::MethodIndex => self.todo.push(Item::Method(v as usize)),
                                Operand::ClassIndex => self.todo.push(Item::Class(v as usize)),
                                _ => {}
                            }
                        }
                    }
                    self.traits(&abc.bodies[bi].traits);
                }
                Item::Class(c) => {
                    if c >= self.classes.len() || self.classes[c] {
                        continue;
                    }
                    self.classes[c] = true;
                    self.todo.push(Item::Method(abc.instances[c].iinit));
                    self.traits(&abc.instances[c].traits);
                    if let Some(cl) = abc.classes.get(c) {
                        self.todo.push(Item::Method(cl.cinit));
                        self.traits(&cl.traits);
                    }
                }
            }
        }
        Some((self.methods, self.classes))
    }
}

/// The private namespaces the given scripts, methods and classes name.
fn private_namespaces(abc: &Abc, code: &[Option<Vec<Instruction>>], scripts: &[usize], methods: &[bool], classes: &[bool]) -> HashSet<usize> {
    let mut mns: Vec<usize> = Vec::new();
    let mut nss: Vec<usize> = Vec::new();
    let traits = |ts: &[Trait], mns: &mut Vec<usize>, nss: &mut Vec<usize>| {
        for t in ts {
            mns.push(t.name);
            if let TraitData::Slot { type_name, value, .. } | TraitData::Const { type_name, value, .. } = &t.data {
                mns.push(*type_name);
                nss.extend(value.filter(|v| is_ns_value(v.kind)).map(|v| v.index));
            }
        }
    };
    for &s in scripts {
        traits(&abc.scripts[s].traits, &mut mns, &mut nss);
    }
    for (c, _) in classes.iter().enumerate().filter(|(_, k)| **k) {
        let i = &abc.instances[c];
        mns.extend([i.name, i.super_name]);
        mns.extend(&i.interfaces);
        nss.push(i.protected_ns);
        traits(&i.traits, &mut mns, &mut nss);
        if let Some(cl) = abc.classes.get(c) {
            traits(&cl.traits, &mut mns, &mut nss);
        }
    }
    for (m, _) in methods.iter().enumerate().filter(|(_, k)| **k) {
        let mi = &abc.methods[m];
        mns.push(mi.return_type);
        mns.extend(&mi.param_types);
        nss.extend(mi.optional.iter().filter(|v| is_ns_value(v.kind)).map(|v| v.index));
        let Some(bi) = abc.body_of.get(m).copied().flatten() else { continue };
        let b = &abc.bodies[bi];
        traits(&b.traits, &mut mns, &mut nss);
        for e in &b.exceptions {
            mns.extend([e.type_name, e.var_name]);
        }
        for i in code[bi].iter().flatten() {
            for (k, &v) in i.kinds.iter().zip(&i.operands) {
                match k {
                    Operand::MultinameIndex => mns.push(v as usize),
                    Operand::NamespaceIndex => nss.push(v as usize),
                    _ => {}
                }
            }
        }
    }
    let mut seen = HashSet::new();
    while let Some(m) = mns.pop() {
        if !seen.insert(m) {
            continue;
        }
        match abc.constants.multinames.get(m) {
            Some(Multiname::QName { ns, .. }) => nss.push(*ns),
            Some(Multiname::Multiname { ns_set, .. } | Multiname::MultinameL { ns_set, .. }) => nss.extend(abc.constants.ns_sets.get(*ns_set).into_iter().flatten()),
            Some(Multiname::TypeName { name, params }) => {
                mns.push(*name);
                mns.extend(params);
            }
            _ => {}
        }
    }
    nss.into_iter().filter(|&n| n != 0 && abc.constants.namespaces.get(n).is_some_and(|n| n.kind == ns_kind::PRIVATE)).collect()
}

fn is_ns_value(kind: u8) -> bool {
    matches!(kind, 0x05 | 0x08 | 0x16..=0x1a)
}

fn index_map(keep: &[bool]) -> Vec<Option<usize>> {
    let mut next = 0;
    keep.iter()
        .map(|&k| {
            k.then(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

/// The ABC with only `scripts` and the kept methods and classes, renumbered in their order. Constants and metadata
/// are copied whole; the encoder writes only what is used.
fn subset(abc: &Abc, scripts: &[usize], keep_m: &[bool], keep_c: &[bool], dialect: Dialect) -> Abc {
    let mmap = index_map(keep_m);
    let cmap = index_map(keep_c);
    let m = |i: usize| mmap.get(i).copied().flatten().unwrap_or(i);
    let fix = |ts: &[Trait]| -> Vec<Trait> {
        let mut ts = ts.to_vec();
        for t in &mut ts {
            match &mut t.data {
                TraitData::Class { class, .. } => *class = cmap.get(*class).copied().flatten().unwrap_or(*class),
                TraitData::Method { method, .. } | TraitData::Getter { method, .. } | TraitData::Setter { method, .. } | TraitData::Function { method, .. } => *method = m(*method),
                _ => {}
            }
        }
        ts
    };
    let mut out = Abc {
        minor: abc.minor,
        major: abc.major,
        constants: abc.constants.clone(),
        methods: abc.methods.iter().zip(keep_m).filter(|(_, k)| **k).map(|(x, _)| x.clone()).collect(),
        metadata: abc.metadata.clone(),
        instances: Vec::new(),
        classes: Vec::new(),
        scripts: scripts.iter().map(|&s| Script { init: m(abc.scripts[s].init), traits: fix(&abc.scripts[s].traits) }).collect(),
        bodies: Vec::new(),
        body_of: Vec::new(),
    };
    for (c, _) in keep_c.iter().enumerate().filter(|(_, k)| **k) {
        let mut i = abc.instances[c].clone();
        i.iinit = m(i.iinit);
        i.traits = fix(&i.traits);
        out.instances.push(i);
        let mut cl = abc.classes[c].clone();
        cl.cinit = m(cl.cinit);
        cl.traits = fix(&cl.traits);
        out.classes.push(cl);
    }
    for b in abc.bodies.iter().filter(|b| keep_m.get(b.method).copied().unwrap_or(false)) {
        let mut b = b.clone();
        b.method = m(b.method);
        b.traits = fix(&b.traits);
        remap_code(&mut b, dialect, &|kind, v| match kind {
            Operand::MethodIndex => mmap.get(v as usize).copied().flatten().map(|x| x as i64),
            Operand::ClassIndex => cmap.get(v as usize).copied().flatten().map(|x| x as i64),
            _ => None,
        });
        out.bodies.push(b);
    }
    out.body_of = vec![None; out.methods.len()];
    for (i, b) in out.bodies.iter().enumerate() {
        if let Some(s) = out.body_of.get_mut(b.method) {
            *s = Some(i);
        }
    }
    out
}

/// Finds or adds constants of another ABC; private namespaces are always added.
#[derive(Default)]
struct Pool {
    strings: HashMap<String, usize>,
    ints: HashMap<i32, usize>,
    uints: HashMap<u32, usize>,
    doubles: HashMap<u64, usize>,
    floats: HashMap<u32, usize>,
    decimals: HashMap<[u8; 16], usize>,
    namespaces: HashMap<Namespace, usize>,
    ns_sets: HashMap<Vec<usize>, usize>,
    multinames: HashMap<Multiname, usize>,
}

fn index_of<K: std::hash::Hash + Eq + Clone, V>(map: &mut HashMap<K, usize>, pool: &mut Vec<V>, key: K, value: V) -> usize {
    *map.entry(key).or_insert_with(|| {
        pool.push(value);
        pool.len() - 1
    })
}

/// Appends `src`'s methods, metadata, classes, scripts and bodies to `dst`, with every constant index remapped.
fn append(dst: &mut Abc, src: &Abc) -> Result<(), ParseError> {
    let c = &dst.constants;
    let mut p = Pool::default();
    for (i, s) in c.strings.iter().enumerate().skip(1) {
        p.strings.entry(s.clone()).or_insert(i);
    }
    for (i, v) in c.ints.iter().enumerate().skip(1) {
        p.ints.entry(*v).or_insert(i);
    }
    for (i, v) in c.uints.iter().enumerate().skip(1) {
        p.uints.entry(*v).or_insert(i);
    }
    for (i, v) in c.doubles.iter().enumerate().skip(1) {
        p.doubles.entry(v.to_bits()).or_insert(i);
    }
    for (i, v) in c.floats.iter().enumerate().skip(1) {
        p.floats.entry(v.to_bits()).or_insert(i);
    }
    for (i, v) in c.decimals.iter().enumerate().skip(1) {
        p.decimals.entry(*v).or_insert(i);
    }
    for (i, v) in c.namespaces.iter().enumerate().skip(1) {
        if v.kind != ns_kind::PRIVATE {
            p.namespaces.entry(*v).or_insert(i);
        }
    }
    for (i, v) in c.ns_sets.iter().enumerate().skip(1) {
        p.ns_sets.entry(v.clone()).or_insert(i);
    }
    for (i, v) in c.multinames.iter().enumerate().skip(1) {
        p.multinames.entry(v.clone()).or_insert(i);
    }
    let s = &src.constants;
    let d = &mut dst.constants;
    let strings: Vec<usize> = s.strings.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.strings, &mut d.strings, v.clone(), v.clone()) }).collect();
    let ints: Vec<usize> = s.ints.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.ints, &mut d.ints, *v, *v) }).collect();
    let uints: Vec<usize> = s.uints.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.uints, &mut d.uints, *v, *v) }).collect();
    let doubles: Vec<usize> = s.doubles.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.doubles, &mut d.doubles, v.to_bits(), *v) }).collect();
    let floats: Vec<usize> = s.floats.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.floats, &mut d.floats, v.to_bits(), *v) }).collect();
    let decimals: Vec<usize> = s.decimals.iter().enumerate().map(|(i, v)| if i == 0 { 0 } else { index_of(&mut p.decimals, &mut d.decimals, *v, *v) }).collect();
    let str_of = |i: usize| strings.get(i).copied().unwrap_or(0);
    let mut namespaces = vec![0; s.namespaces.len()];
    for (i, n) in s.namespaces.iter().enumerate().skip(1) {
        let n = Namespace { kind: n.kind, name: str_of(n.name) };
        namespaces[i] = if n.kind == ns_kind::PRIVATE {
            d.namespaces.push(n);
            d.namespaces.len() - 1
        } else {
            index_of(&mut p.namespaces, &mut d.namespaces, n, n)
        };
    }
    let ns_of = |i: usize| namespaces.get(i).copied().unwrap_or(0);
    let ns_sets: Vec<usize> = s
        .ns_sets
        .iter()
        .enumerate()
        .map(|(i, v)| {
            if i == 0 {
                return 0;
            }
            let v: Vec<usize> = v.iter().map(|&n| ns_of(n)).collect();
            index_of(&mut p.ns_sets, &mut d.ns_sets, v.clone(), v)
        })
        .collect();
    let set_of = |i: usize| ns_sets.get(i).copied().unwrap_or(0);
    let mut multinames: Vec<Option<usize>> = vec![None; s.multinames.len()];
    multinames[0] = Some(0);
    fn mn(i: usize, s: &[Multiname], out: &mut Vec<Option<usize>>, map: &dyn Fn(&Multiname, &[Option<usize>]) -> Multiname, add: &mut dyn FnMut(Multiname) -> usize, depth: usize) -> usize {
        if let Some(Some(v)) = out.get(i) {
            return *v;
        }
        let Some(m) = s.get(i) else { return 0 };
        if depth > 64 {
            return 0;
        }
        if let Multiname::TypeName { name, params } = m {
            for &k in std::iter::once(name).chain(params) {
                mn(k, s, out, map, add, depth + 1);
            }
        }
        let v = add(map(m, out));
        out[i] = Some(v);
        v
    }
    let map = |m: &Multiname, done: &[Option<usize>]| -> Multiname {
        let r = |k: usize| done.get(k).copied().flatten().unwrap_or(0);
        match m {
            Multiname::QName { attr, ns, name } => Multiname::QName { attr: *attr, ns: ns_of(*ns), name: str_of(*name) },
            Multiname::RtqName { attr, name } => Multiname::RtqName { attr: *attr, name: str_of(*name) },
            Multiname::RtqNameL { attr } => Multiname::RtqNameL { attr: *attr },
            Multiname::Multiname { attr, name, ns_set } => Multiname::Multiname { attr: *attr, name: str_of(*name), ns_set: set_of(*ns_set) },
            Multiname::MultinameL { attr, ns_set } => Multiname::MultinameL { attr: *attr, ns_set: set_of(*ns_set) },
            Multiname::TypeName { name, params } => Multiname::TypeName { name: r(*name), params: params.iter().map(|&k| r(k)).collect() },
        }
    };
    let mut add = |m: Multiname| index_of(&mut p.multinames, &mut d.multinames, m.clone(), m);
    for i in 1..s.multinames.len() {
        mn(i, &s.multinames, &mut multinames, &map, &mut add, 0);
    }
    let mn_of = |i: usize| multinames.get(i).copied().flatten().unwrap_or(0);
    let decimal = src.minor >= 17;
    let value = |v: ValueKind| -> ValueKind {
        let index = match v.kind {
            0x01 => str_of(v.index),
            0x02 if decimal => decimals.get(v.index).copied().unwrap_or(0),
            0x02 => floats.get(v.index).copied().unwrap_or(0),
            0x03 => ints.get(v.index).copied().unwrap_or(0),
            0x04 => uints.get(v.index).copied().unwrap_or(0),
            0x06 => doubles.get(v.index).copied().unwrap_or(0),
            k if is_ns_value(k) => ns_of(v.index),
            _ => v.index,
        };
        ValueKind { index, kind: v.kind }
    };
    let (moff, coff, mdoff) = (dst.methods.len(), dst.instances.len(), dst.metadata.len());
    let traits = |ts: &[Trait]| -> Vec<Trait> {
        ts.iter()
            .map(|t| {
                let data = match &t.data {
                    TraitData::Slot { slot_id, type_name, value: v } => TraitData::Slot { slot_id: *slot_id, type_name: mn_of(*type_name), value: v.map(value) },
                    TraitData::Const { slot_id, type_name, value: v } => TraitData::Const { slot_id: *slot_id, type_name: mn_of(*type_name), value: v.map(value) },
                    TraitData::Method { disp_id, method } => TraitData::Method { disp_id: *disp_id, method: method + moff },
                    TraitData::Getter { disp_id, method } => TraitData::Getter { disp_id: *disp_id, method: method + moff },
                    TraitData::Setter { disp_id, method } => TraitData::Setter { disp_id: *disp_id, method: method + moff },
                    TraitData::Class { slot_id, class } => TraitData::Class { slot_id: *slot_id, class: class + coff },
                    TraitData::Function { slot_id, method } => TraitData::Function { slot_id: *slot_id, method: method + moff },
                };
                Trait { name: mn_of(t.name), attrs: t.attrs, data, metadata: t.metadata.iter().map(|m| m + mdoff).collect() }
            })
            .collect()
    };
    for m in &src.methods {
        let mut m = m.clone();
        m.return_type = mn_of(m.return_type);
        m.param_types = m.param_types.iter().map(|&t| mn_of(t)).collect();
        m.name = str_of(m.name);
        m.optional = m.optional.iter().map(|&v| value(v)).collect();
        m.param_names = m.param_names.iter().map(|&n| str_of(n)).collect();
        dst.methods.push(m);
    }
    for md in &src.metadata {
        let mut md = md.clone();
        md.name = str_of(md.name);
        md.keys = md.keys.iter().map(|&k| str_of(k)).collect();
        md.values = md.values.iter().map(|&v| str_of(v)).collect();
        dst.metadata.push(md);
    }
    for (i, cl) in src.instances.iter().zip(&src.classes) {
        let mut i = i.clone();
        i.name = mn_of(i.name);
        i.super_name = mn_of(i.super_name);
        i.protected_ns = ns_of(i.protected_ns);
        i.interfaces = i.interfaces.iter().map(|&n| mn_of(n)).collect();
        i.iinit += moff;
        i.traits = traits(&i.traits);
        dst.instances.push(i);
        dst.classes.push(crate::abc::Class { cinit: cl.cinit + moff, traits: traits(&cl.traits) });
    }
    for sc in &src.scripts {
        dst.scripts.push(Script { init: sc.init + moff, traits: traits(&sc.traits) });
    }
    let dialect = Dialect::of(src.major, src.minor);
    for b in &src.bodies {
        let mut b = b.clone();
        decode(&b.code, dialect).map_err(|e| fail(format!("the compiled code cannot be decoded: {e}")))?;
        b.method += moff;
        b.traits = traits(&b.traits);
        b.exceptions = b.exceptions.iter().map(|e| Exception { type_name: mn_of(e.type_name), var_name: mn_of(e.var_name), ..e.clone() }).collect();
        remap_code(&mut b, dialect, &|kind, v| {
            let v = v as usize;
            Some((match kind {
                Operand::MultinameIndex => mn_of(v),
                Operand::StringIndex => str_of(v),
                Operand::NamespaceIndex => ns_of(v),
                Operand::IntIndex => ints.get(v).copied().unwrap_or(0),
                Operand::UintIndex => uints.get(v).copied().unwrap_or(0),
                Operand::DoubleIndex => doubles.get(v).copied().unwrap_or(0),
                Operand::FloatIndex => floats.get(v).copied().unwrap_or(0),
                Operand::DecimalIndex => decimals.get(v).copied().unwrap_or(0),
                Operand::MethodIndex => v + moff,
                Operand::ClassIndex => v + coff,
                _ => return None,
            }) as i64)
        });
        dst.bodies.push(b);
    }
    dst.body_of = vec![None; dst.methods.len()];
    for (i, b) in dst.bodies.iter().enumerate() {
        if let Some(s) = dst.body_of.get_mut(b.method) {
            *s = Some(i);
        }
    }
    Ok(())
}

/// Re-encodes a body with some operands replaced, keeping branches and exceptions pointing at the same instructions.
fn remap_code(b: &mut crate::abc::MethodBody, dialect: Dialect, f: &dyn Fn(Operand, i64) -> Option<i64>) {
    let Ok(code) = decode(&b.code, dialect) else { return };
    let mut out = code.clone();
    let mut changed = false;
    for i in &mut out {
        for k in 0..i.kinds.len().min(i.operands.len()) {
            if let Some(v) = f(i.kinds[k], i.operands[k]) {
                if v != i.operands[k] {
                    i.operands[k] = v;
                    changed = true;
                }
            }
        }
    }
    if changed {
        let keep: Vec<usize> = (0..out.len()).collect();
        let _ = reencode(b, &code, &out, &keep);
    }
}

/// Writes the instructions of `out` at `keep` (indexes into `code`, the body's decoded form), moving branch targets
/// and exception ranges with their instructions.
fn reencode(b: &mut crate::abc::MethodBody, code: &[Instruction], out: &[Instruction], keep: &[usize]) -> Option<()> {
    let lens: Vec<usize> = out.iter().map(|i| crate::abc::asm::encode(i).len()).collect();
    let mut new_off = vec![0usize; code.len() + 1];
    let mut a = 0;
    let mut k = 0;
    for p in 0..code.len() {
        new_off[p] = a;
        if keep.get(k) == Some(&p) {
            a += lens[p];
            k += 1;
        }
    }
    new_off[code.len()] = a;
    let total = code.last().map_or(0, |i| i.offset + i.len);
    let map = |old: i64| -> Option<i64> {
        if old as usize == total {
            return Some(a as i64);
        }
        code.binary_search_by_key(&(old as usize), |i| i.offset).ok().map(|p| new_off[p] as i64)
    };
    let mut bytes = Vec::new();
    for &p in keep {
        let old = &code[p];
        let mut i = out[p].clone();
        for k in 0..i.kinds.len() {
            match i.kinds[k] {
                Operand::Offset => i.operands[k] = map(old.offset as i64 + old.len as i64 + old.operands[k])? - (new_off[p] + lens[p]) as i64,
                Operand::CaseBaseoffset => i.operands[k] = map(old.offset as i64 + old.operands[k])? - new_off[p] as i64,
                Operand::OptCaseOffsets => {
                    for q in k + 1..i.operands.len() {
                        i.operands[q] = map(old.offset as i64 + old.operands[q])? - new_off[p] as i64;
                    }
                }
                _ => {}
            }
        }
        bytes.extend(crate::abc::asm::encode(&i));
    }
    let mut exceptions = b.exceptions.clone();
    for e in &mut exceptions {
        for v in [&mut e.start, &mut e.end, &mut e.target] {
            *v = map(*v as i64)? as usize;
        }
    }
    b.code = bytes;
    b.exceptions = exceptions;
    Some(())
}

/// Drops the instructions a script initializer uses to create class `class`: the global object or
/// `findpropstrict`, the ancestor scopes, `newclass`, the `popscope`s and the store.
fn strip_class_init(b: &mut crate::abc::MethodBody, class: usize, dialect: Dialect) -> Option<()> {
    let code = decode(&b.code, dialect).ok()?;
    let k = code.iter().position(|i| i.code == 0x58 && i.operands[0] as usize == class)?;
    let mut e = k + 1;
    while code.get(e)?.code == 0x1d {
        e += 1;
    }
    if !matches!(code[e].code, 0x68 | 0x61 | 0x6d) {
        return None;
    }
    let mut j = k.checked_sub(1)?;
    while matches!(code[j].code, 0x60 | 0x30 | 0x2a | 0x20) {
        j = j.checked_sub(1)?;
    }
    if !matches!(code[j].code, 0x5d | 0x5e | 0x65 | 0x64) {
        return None;
    }
    let keep: Vec<usize> = (0..code.len()).filter(|&p| p < j || p > e).collect();
    reencode(b, &code, &code, &keep)
}
