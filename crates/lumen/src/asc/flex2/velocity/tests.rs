//! Checks against flex2's Velocity: AST dumps of mxmlc.jar's .vms files, init logs and recorded and live renderings.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::introspection::Info;
use super::node::{Kind, Node};
use super::parser::JJT_NODE_NAME;
use super::runtime::RuntimeInstance;
use super::template::Template;

thread_local! {
    /// Nodes rendered so far, for the oracle's block coverage report.
    pub(super) static RENDERED: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
}

/// Lines of the blocks under `n` that were never rendered.
pub(super) fn unrendered_blocks(n: &Rc<Node>, out: &mut Vec<i32>) {
    if n.id == super::parser::JJTBLOCK && !RENDERED.with(|r| r.borrow().contains(&n.key())) {
        out.push(n.begin_line);
        return;
    }
    for c in &n.children {
        unrendered_blocks(c, out);
    }
}

fn esc(s: &str) -> String {
    let mut b = String::from("\"");
    for u in crate::asc::util::jstr::units(s) {
        match u {
            10 => b.push_str("\\n"),
            13 => b.push_str("\\r"),
            9 => b.push_str("\\t"),
            0x5C => b.push_str("\\\\"),
            0x22 => b.push_str("\\\""),
            c if !(32..=126).contains(&c) => b.push_str(&format!("\\u{:04x}", c)),
            c => b.push(c as u8 as char),
        }
    }
    b.push('"');
    b
}

fn opt(s: &Option<String>) -> String {
    s.as_deref().map(esc).unwrap_or_else(|| "null".to_string())
}

fn info(i: &Option<Info>) -> String {
    match i {
        None => "null".to_string(),
        Some(i) => format!("Info({},{},{})", esc(&i.template_name), i.line, i.column),
    }
}

/// The serialized state, in the format of the Java `Dump` harness.
pub(super) fn dump(n: &Rc<Node>, indent: &str, out: &mut String) {
    let mut f: Vec<(&str, String)> = vec![
        ("beginColumn", n.begin_column.to_string()),
        ("beginLine", n.begin_line.to_string()),
        ("firstTokenImage", opt(&n.first_token_image)),
        ("id", n.id.to_string()),
        ("info", "0".to_string()),
        ("invalid", "false".to_string()),
        ("state", "false".to_string()),
    ];
    let mut tree = None;
    match &n.kind {
        Kind::Plain => {}
        Kind::Text(t) => f.push(("text", esc(t))),
        Kind::Comment(c) => f.push(("comment", esc(c))),
        Kind::Escape(v) => f.push(("val", esc(v))),
        Kind::Identifier { identifier, uber_info } => {
            f.push(("identifier", esc(identifier)));
            f.push(("uberInfo", info(&uber_info.borrow())));
        }
        Kind::Method { method_name, .. } => f.push(("methodName", esc(method_name))),
        Kind::NumberLiteral(v) => f.push(("value", v.to_string())),
        Kind::StringLiteral { image, interpolate, node_tree } => {
            f.push(("image", esc(image)));
            f.push(("interpolate", interpolate.get().to_string()));
            tree = node_tree.borrow().clone();
        }
        Kind::Directive { directive_name, .. } => f.push(("directiveName", esc(directive_name))),
        Kind::Reference { info: r, uber_info } => {
            f.push(("computableReference", r.computable_reference.to_string()));
            f.push(("escPrefix", esc(&r.esc_prefix)));
            f.push(("escaped", r.escaped.to_string()));
            f.push(("identifier", opt(&r.identifier)));
            f.push(("literal", opt(&r.literal)));
            f.push(("morePrefix", esc(&r.more_prefix)));
            f.push(("nullString", esc(&r.null_string)));
            f.push(("numChildren", r.num_children.to_string()));
            f.push(("referenceType", r.reference_type.to_string()));
            f.push(("rootString", esc(&r.root_string)));
            f.push(("uberInfo", info(&uber_info.borrow())));
        }
        Kind::SetDirective { left_reference, .. } => {
            let p = "org.apache.flex.forks.velocity.runtime.parser.node.AST";
            f.push(("left", format!("<{p}Reference>")));
            f.push(("leftReference", esc(left_reference)));
            f.push(("right", format!("<{p}{}>", JJT_NODE_NAME[n.children[1].id as usize])));
        }
    }
    f.sort_by(|a, b| a.0.cmp(b.0));
    out.push_str(indent);
    out.push_str("AST");
    out.push_str(JJT_NODE_NAME[n.id as usize]);
    for (k, v) in f {
        out.push(' ');
        out.push_str(k);
        out.push('=');
        out.push_str(&v);
    }
    out.push('\n');
    if let Some(t) = tree {
        out.push_str(indent);
        out.push_str(" nodeTree:\n");
        dump(&t, &format!("{indent}  "), out);
    }
    for c in &n.children {
        dump(c, &format!("{indent} "), out);
    }
}

const FLEX_TEMPLATES: &[&str] = &[
    "flex2/compiler/as3/EmbedClass.vm",
    "flex2/compiler/as3/binding/BindableProperty.vm",
    "flex2/compiler/as3/binding/WatcherSetupUtil.vm",
    "flex2/compiler/as3/managed/ManagedProperty.vm",
    "flex2/compiler/css/FontFaceRules.vm",
    "flex2/compiler/css/StyleDef.vm",
    "flex2/compiler/css/StyleLibrary.vm",
    "flex2/compiler/css/StyleModule.vm",
    "flex2/compiler/media/SkinClass.vm",
    "flex2/compiler/mxml/gen/ClassDef.vm",
    "flex2/compiler/mxml/gen/ClassDefLib.vm",
    "flex2/compiler/mxml/gen/InterfaceDef.vm",
];

/// Compares the built trees with dumps of mxmlc.jar's `.vms` files (`VELOCITY_ORACLE/vmsdump/<Name>.dump`).
#[test]
fn bundled_templates_match_vms() {
    let Ok(dir) = std::env::var("VELOCITY_ORACLE") else { return };
    let build = RuntimeInstance::new(None, None);
    let mut bad = Vec::new();
    for path in FLEX_TEMPLATES {
        let src = super::velocity_manager::tests_source(path);
        let t = Template::build(&src, path, &build).unwrap();
        let mut out = format!("== {path}s\n");
        dump(t.data(), "", &mut out);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".vm");
        let expected = std::fs::read_to_string(format!("{dir}/vmsdump/{name}.dump")).unwrap().replace("\r\n", "\n");
        if out != expected {
            std::fs::write(format!("{dir}/vmsdump/{name}.rust"), &out).unwrap();
            bad.push(name.to_string());
        }
    }
    assert!(bad.is_empty(), "differ: {bad:?}");
}

/// The warn/error log of initializing each template (after its lib) in a fresh flex-configured engine, as `velo.InitLog`
/// prints it (`VELOCITY_ORACLE/initlog.java.txt`).
#[test]
fn init_log_matches_java() {
    let Ok(dir) = std::env::var("VELOCITY_ORACLE") else { return };
    let runs: Vec<Vec<&str>> = vec![
        vec![FLEX_TEMPLATES[0]],
        vec![FLEX_TEMPLATES[1]],
        vec![FLEX_TEMPLATES[2]],
        vec![FLEX_TEMPLATES[3]],
        vec![FLEX_TEMPLATES[4]],
        vec![FLEX_TEMPLATES[5]],
        vec![FLEX_TEMPLATES[6]],
        vec![FLEX_TEMPLATES[7]],
        vec![FLEX_TEMPLATES[8]],
        vec![FLEX_TEMPLATES[10], FLEX_TEMPLATES[9]],
        vec![FLEX_TEMPLATES[11]],
        vec![FLEX_TEMPLATES[6], FLEX_TEMPLATES[7]],
    ];
    let log = Rc::new(RefCell::new(String::new()));
    let build = RuntimeInstance::new(None, None);
    for run in runs {
        log.borrow_mut().push_str(&format!("== {}\n", run.join("+")));
        let l = log.clone();
        let ve = RuntimeInstance::new(
            Some(Box::new(|_| {})),
            Some(Box::new(move |level, m| {
                if level >= 2 {
                    l.borrow_mut().push_str(&format!("  [{level}] {m}\n"));
                }
            })),
        );
        for path in run {
            let t = Template::build(&super::velocity_manager::tests_source(path), path, &build).unwrap();
            t.set_runtime_services(&ve);
            t.init_document().unwrap();
        }
    }
    let expected = std::fs::read_to_string(format!("{dir}/initlog.java.txt")).unwrap().replace("\r\n", "\n");
    assert_eq!(*log.borrow(), expected);
}

/// Outputs recorded from mxmlc.jar's VelocityManager for templates whose context needs no Java objects.
#[test]
#[rustfmt::skip]
fn renders_recorded_java_output() {
    use super::introspection::Value;
    use super::{get_code_gen_context, get_template, register_template_source};
    let cases: Vec<(&str, Option<&str>, Vec<(&str, Value)>, &str)> = vec![
    (r#"vtl/comments.vm"#, Some(r#"a ## line comment
b #* block
comment *# c
d #** doc
comment *# e
f #*# odd *# g
h ## #* tricky
still comment? *# i
j $name## after ref
k #* nested #* inner *# outer *# l
m ##
n #**#
o #***# p
"#), vec![("name", Value::str(r#"World"#)), ("list", Value::list(vec![Value::str(r#"a"#), Value::str(r#"b"#), Value::str(r#"c"#)])), ("map", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"a"#), Value::Integer(1)), (Value::str(r#"b"#), Value::str(r#"two"#))])), ("iter", Value::iterator(vec![Value::Integer(1), Value::Integer(2)])), ("empty", Value::list(vec![])), ("lo", Value::Integer(2)), ("hi", Value::Integer(4)), ("t", Value::Boolean(true)), ("f", Value::Boolean(false)), ("long", Value::Long(123)), ("n", Value::Integer(3))], r#"a b  c
d  e
f  g
h j Worldk  l
m n 
o  p
"#),
    (r#"vtl/foreach.vm"#, Some(r#"#set($velocityCount = "outer")
#foreach ($a in [1, 2, 3])
a=$a c=$velocityCount
#foreach ($b in ["x", "y"])
  b=$b c=$velocityCount a=$a
#end
after inner c=$velocityCount
#end
done c=$velocityCount a=$a
#foreach ($i in [3..1])$i #end
#foreach ($i in [$lo..$hi])$i #end
#foreach ($i in [-2..2])$i #end
#foreach ($e in $empty)never#end
#foreach ($e in $nope)never#end
#foreach ($v in $map)$v;#end
#foreach ($k in $map.keySet())$k=$map.get($k) #end
#foreach ($en in $map.entrySet())$en.key:$en.value $en.getKey() #end
#foreach ($x in $iter)$x #end|#foreach ($x in $iter)again#end
#set($keep = "kept")
#foreach ($keep in ["k1"])$keep#end $keep
#foreach ($s in $name)$s#end
#foreach ($n in [1, $nope, "z"])[$n]#end
#foreach($q in $list)$velocityCount:$q #end
"#), vec![("name", Value::str(r#"World"#)), ("list", Value::list(vec![Value::str(r#"a"#), Value::str(r#"b"#), Value::str(r#"c"#)])), ("map", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"a"#), Value::Integer(1)), (Value::str(r#"b"#), Value::str(r#"two"#))])), ("iter", Value::iterator(vec![Value::Integer(1), Value::Integer(2)])), ("empty", Value::list(vec![])), ("lo", Value::Integer(2)), ("hi", Value::Integer(4)), ("t", Value::Boolean(true)), ("f", Value::Boolean(false)), ("long", Value::Long(123)), ("n", Value::Integer(3))], r#"a=1 c=1
  b=x c=1 a=1
  b=y c=2 a=1
after inner c=1
a=2 c=2
  b=x c=1 a=2
  b=y c=2 a=2
after inner c=2
a=3 c=3
  b=x c=1 a=3
  b=y c=2 a=3
after inner c=3
done c=outer a=$a
3 2 1 2 3 4 -2 -1 0 1 2 1;two;a=1 b=two a:1 a b:two b 1 2 |k1 kept
[1][1][z]1:a 2:b 3:c "#),
    (r#"vtl/whitespace.vm"#, Some(r#"#if ($t)   
   inside
#end   
#if ($t)	
tab
#else   
no
#end
  #if ($t)keep-leading#end  
#foreach ($i in [1..2])  
  item $i  
#end  
#set($x = 1)   
	#set($y = 2)	
z $x $y
#if ($t) #end
end
"#), vec![("name", Value::str(r#"World"#)), ("list", Value::list(vec![Value::str(r#"a"#), Value::str(r#"b"#), Value::str(r#"c"#)])), ("map", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"a"#), Value::Integer(1)), (Value::str(r#"b"#), Value::str(r#"two"#))])), ("iter", Value::iterator(vec![Value::Integer(1), Value::Integer(2)])), ("empty", Value::list(vec![])), ("lo", Value::Integer(2)), ("hi", Value::Integer(4)), ("t", Value::Boolean(true)), ("f", Value::Boolean(false)), ("long", Value::Long(123)), ("n", Value::Integer(3))], r#"   inside
tab
  keep-leading  item 1  
  item 2  
z 1 2
 end
"#),
    (r#"vtl/escapes.vm"#, Some(r#"1 \$name \\$name \\\$name \\\\$name
2 \$nope \\$nope \\\$nope
3 $!nope \$!nope \$!name $\!name $\\!name
4 ${name} \${name} $!{name} $!{nope} \$!{nope}
5 \#if \#end \#foreach \#mac \#unknown \#set \#else \#stop \#elseif
6 \\#if(true)yes#end and \\\\#if(false)no#else no2#end
7 # hello #1 #@x a#,b #
8 cost $5 and $ alone, $-x $_ok $a-b-c $a_b
9 $name. $name.1 $name.. ${name}.x $name.length() $name.nope
10 email@host.com 100% \n \t \\ \x
11 trailing dollar $
12 $name$name $name#if(true)T#end ${name}${name}
13 \\\\\#if
14 a\\b \\\\x \\\\\\
#macro(mac $a)[$a]#end
15 \#mac \\#mac('m') #mac('n') \\\#mac
16 \\\#if \\\#end \\\\\#foreach
17 \\$!nope \\\$!nope $\\\!name \$\!name \\$!{name}
"#), vec![("name", Value::str(r#"World"#)), ("list", Value::list(vec![Value::str(r#"a"#), Value::str(r#"b"#), Value::str(r#"c"#)])), ("map", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"a"#), Value::Integer(1)), (Value::str(r#"b"#), Value::str(r#"two"#))])), ("iter", Value::iterator(vec![Value::Integer(1), Value::Integer(2)])), ("empty", Value::list(vec![])), ("lo", Value::Integer(2)), ("hi", Value::Integer(4)), ("t", Value::Boolean(true)), ("f", Value::Boolean(false)), ("long", Value::Long(123)), ("n", Value::Integer(3))], r#"1 $name \World \$name \\World
2 \$nope \\$nope \\$nope
3  \$!nope $!name $!name $\!name
4 World ${name} World  \$!{nope}
5 \#if \#end \#foreach \#mac \#unknown \#set \#else \#stop \#elseif
6 \yes and \\ no27 # hello #1 #@x a#,b #
8 cost $5 and $ alone, $-x $_ok $a-b-c $a_b
9 World. World.1 World.. World.x 5 $name.nope
10 email@host.com 100% \n \t \\ \x
11 trailing dollar $
12 WorldWorld WorldT WorldWorld
13 \\\\\#if
14 a\\b \\\\x \\\\\\
15 \#mac \[m] [n] \\\#mac
16 \\\#if \\\#end \\\\\#foreach
17 \\ \\$!nope $\\!name \$!name \World
"#),
    (r#"vtl/basic.vm"#, Some(r#"Hello $name! ${name}x $!missing $missing
#set($a = 3)
#if ($a == 3)three#elseif ($a > 3)more#else less#end
#foreach ($i in [1..3])$i:$velocityCount #end
"#), vec![("name", Value::str(r#"World"#)), ("list", Value::list(vec![Value::str(r#"a"#), Value::str(r#"b"#), Value::str(r#"c"#)])), ("map", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"a"#), Value::Integer(1)), (Value::str(r#"b"#), Value::str(r#"two"#))])), ("iter", Value::iterator(vec![Value::Integer(1), Value::Integer(2)])), ("empty", Value::list(vec![])), ("lo", Value::Integer(2)), ("hi", Value::Integer(4)), ("t", Value::Boolean(true)), ("f", Value::Boolean(false)), ("long", Value::Long(123)), ("n", Value::Integer(3))], r#"Hello World! Worldx  $missing
three1:1 2:2 3:3 "#),
    (r#"flex2/compiler/as3/EmbedClass.vm"#, None, vec![("packageName", Value::str(r#"a.b"#)), ("baseClass", Value::str(r#"mx.core.BitmapAsset"#)), ("assetType", Value::str(r#"flash.display.Bitmap"#)), ("embedClass", Value::str(r#"A_png"#)), ("embedMap", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"source"#), Value::str(r#"a.png"#)), (Value::str(r#"mimeType"#), Value::str(r#"image/png"#)), (Value::str(r#"smoothing"#), Value::str(r#"true"#))])), ("embedProps", Value::map_of("java.util.LinkedHashMap", vec![(Value::str(r#"p1"#), Value::str(r#"String"#)), (Value::str(r#"p2"#), Value::str(r#"int"#))]))], r#"

package a.b
{

import mx.core.BitmapAsset;
import flash.display.Bitmap;

[ExcludeClass]
[Embed(source="a.png", mimeType="image/png", smoothing="true")]

public class A_png extends mx.core.BitmapAsset 
{
    public function A_png() 
    { 
	    super(); 
    }

    public var p1:String;
    public var p2:int;
}

}
"#),
    (r#"flex2/compiler/media/SkinClass.vm"#, None, vec![("packageName", Value::str(r#"p"#)), ("className", Value::str(r#"Half"#)), ("baseClassName", Value::str(r#"B"#)), ("needsIBorder", Value::Boolean(true)), ("needsBorderMetrics", Value::Boolean(false)), ("needsName", Value::Boolean(true)), ("needsMeasuredWidth", Value::Boolean(false)), ("needsMeasuredHeight", Value::Boolean(true)), ("needsIFlexDisplayObject", Value::Boolean(false)), ("needsMove", Value::Boolean(true)), ("needsSetActualSize", Value::Boolean(false)), ("needsToString", Value::Boolean(true))], r#"

package p
{
import B;
import mx.core.IBorder;
import mx.core.IFlexDisplayObject;
import mx.core.IFlexAsset;
import mx.utils.NameUtil;

[ExcludeClass]
public class Half extends B
    implements IBorder, IFlexAsset, IFlexDisplayObject
{
    public function Half()
    {
        super();

        try
        {
            name = NameUtil.createUniqueName(this);
        }
        catch(e:Error)
        {
        }

        _measuredHeight = height;
    }



    override public function toString():String
    {
        return NameUtil.displayObjectToString(this);
    }
}
}"#),
    ];
    for (path, src, ctx, expected) in cases {
        if let Some(src) = src {
            register_template_source(path, src);
        }
        let t = get_template(path).unwrap();
        let vc = get_code_gen_context(None);
        for (k, v) in ctx {
            vc.put(k, v);
        }
        assert_eq!(t.merge(&vc).unwrap(), expected, "{path}");
    }
}

mod oracle;
