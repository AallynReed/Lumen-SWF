use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use lumen::export::{Exporter, Kind, Options};
use lumen::tree::{self, Node};
use lumen::{Compression, SaveOptions, Swf};

#[derive(Parser)]
#[command(name = "lumen-swf-cli", version = option_env!("LUMEN_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")), about = "Lumen-SWF command line")]
struct Cli {
    /// Remove obfuscation traps from scripts before decompiling
    #[arg(long, global = true)]
    deobfuscate: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compile an AS3 application into a SWF, taking mxmlc's own arguments
    #[command(after_help = "Takes the arguments mxmlc.jar takes, for example:
  lumen-swf-cli mxmlc -load-config= -external-library-path=playerglobal.swc -target-player=11.2 -swf-version=15 -default-size 601 502 -default-frame-rate 24 -default-background-color=0 -source-path src lib -output out.swf src/Main.as")]
    Mxmlc {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Print the header and any parse warnings
    #[command(after_help = "Examples:
  lumen-swf-cli info movie.swf")]
    Info { file: PathBuf },
    /// Print the resource tree, or the raw tag list with --raw
    #[command(after_help = "Examples:
  lumen-swf-cli tags movie.swf
  lumen-swf-cli tags movie.swf --raw")]
    Tags {
        file: PathBuf,
        #[arg(long)]
        raw: bool,
    },
    /// Print the fields of one tag, addressed by path such as 12 or 12/3
    #[command(after_help = "Examples:
  lumen-swf-cli tag movie.swf 12
  lumen-swf-cli tag movie.swf 12/3")]
    Tag { file: PathBuf, path: String },
    /// Write the raw body of one tag to a file
    #[command(after_help = "Examples:
  lumen-swf-cli extract-tag movie.swf 12 body.bin")]
    ExtractTag { file: PathBuf, path: String, out: PathBuf },
    /// Export assets into <outdir>/<kind> folders
    #[command(after_help = "Examples:
  lumen-swf-cli export movie.swf outdir
  lumen-swf-cli export movie.swf outdir --kinds shapes,sprites --png
  lumen-swf-cli export movie.swf outdir --kinds scripts --pcode
  lumen-swf-cli export movie.swf outdir --kinds frames --frames-svg --zoom 2")]
    Export {
        file: PathBuf,
        outdir: PathBuf,
        /// Comma-separated kinds: images, sounds, binaryData, fonts, texts, movies, shapes,
        /// morphshapes, frames, sprites, buttons, scripts (default all)
        #[arg(long, value_delimiter = ',')]
        kinds: Vec<String>,
        /// Write every image as PNG
        #[arg(long)]
        png: bool,
        /// Decode every sound to WAV
        #[arg(long)]
        wav: bool,
        /// Shapes and morph shapes as PNG instead of SVG
        #[arg(long)]
        shapes_png: bool,
        /// Frames, sprites and buttons as SVG instead of PNG
        #[arg(long)]
        frames_svg: bool,
        /// Texts as SVG instead of plain text
        #[arg(long)]
        texts_svg: bool,
        /// Raster scale for PNG output
        #[arg(long, default_value_t = 1.0)]
        zoom: f32,
        /// Scripts as p-code instead of decompiled ActionScript
        #[arg(long)]
        pcode: bool,
    },
    /// Render a character (by id) or a root frame (frame:N, 1-based) to .svg or .png
    #[command(after_help = "Examples:
  lumen-swf-cli render movie.swf 12 shape.svg
  lumen-swf-cli render movie.swf frame:5 frame.png --zoom 2")]
    Render {
        file: PathBuf,
        what: String,
        out: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        zoom: f32,
    },
    /// Print the display list of a root frame (1-based)
    #[command(after_help = "Examples:
  lumen-swf-cli frame movie.swf 5")]
    Frame { file: PathBuf, frame: usize },
    /// List AS3 scripts (classes and package-level definitions) and AS1/2 scripts (export paths)
    #[command(after_help = "Examples:
  lumen-swf-cli scripts movie.swf")]
    Scripts { file: PathBuf },
    /// Print the p-code of a script: an AS3 full name such as com.example.Main, or an AS1/2
    /// export path such as frame_1/DoAction
    #[command(after_help = "Examples:
  lumen-swf-cli pcode movie.swf com.example.Main
  lumen-swf-cli pcode movie.swf com.example.Main --method \"instance initializer\"
  lumen-swf-cli pcode movie.swf frame_1/DoAction")]
    Pcode {
        file: PathBuf,
        name: String,
        /// One AS3 method, such as "instance initializer", "foo" or "static get bar"
        #[arg(long)]
        method: Option<String>,
    },
    /// Print the decompiled ActionScript of an AS3 script by full name
    #[command(after_help = "Examples:
  lumen-swf-cli as3 movie.swf com.example.Main
  lumen-swf-cli as3 movie.swf com.example.Main --deobfuscate")]
    As3 { file: PathBuf, name: String },
    /// Print the decompiled ActionScript of an AS1/2 script by export path, e.g. frame_1/DoAction
    #[command(after_help = "Examples:
  lumen-swf-cli as2 movie.swf frame_1/DoAction")]
    As2 { file: PathBuf, name: String },
    /// Replace a character (by id) with a file, or a script (by name) with ActionScript or
    /// .pcode
    #[command(after_help = "Examples:
  lumen-swf-cli replace in.swf out.swf 4 image.png
  lumen-swf-cli replace in.swf out.swf 7 shape.svg
  lumen-swf-cli replace in.swf out.swf 2 font.ttf --chars \"àéü€\"
  lumen-swf-cli replace in.swf out.swf frame_1/DoAction script.as
  lumen-swf-cli replace in.swf out.swf com.example.Main Main.as
  lumen-swf-cli replace in.swf out.swf com.example.Main method.pcode --method foo
  lumen-swf-cli replace in.swf out.swf 12/3 body.bin --raw")]
    Replace {
        input: PathBuf,
        output: PathBuf,
        target: String,
        data: PathBuf,
        /// The AS3 method to replace, as named by `pcode --method`
        #[arg(long)]
        method: Option<String>,
        /// Replace the raw body of the tag at path TARGET, such as 12 or 12/3
        #[arg(long)]
        raw: bool,
        /// Characters to add to a DefineFont2/3 from the font file, besides those it has
        #[arg(long)]
        chars: Option<String>,
    },
    /// List the SWFs in a ZIP, SWC or any other file, or extract them into OUTDIR
    #[command(after_help = "Examples:
  lumen-swf-cli bundle game.exe
  lumen-swf-cli bundle game.exe outdir")]
    Bundle { file: PathBuf, outdir: Option<PathBuf> },
    /// Export an AS3 SWF as an SWC library
    #[command(after_help = "Examples:
  lumen-swf-cli swc movie.swf movie.swc
  lumen-swf-cli swc movie.swf movie.swc --no-deps")]
    Swc {
        input: PathBuf,
        output: PathBuf,
        /// Leave the dependencies out of catalog.xml
        #[arg(long)]
        no_deps: bool,
    },
    /// Convert a SWF to an XFL folder (or a zipped FLA with --compressed), named after OUTPUT.fla
    #[command(after_help = "Examples:
  lumen-swf-cli xfl movie.swf movie.fla
  lumen-swf-cli xfl movie.swf movie.fla --compressed --version cs6")]
    Xfl {
        input: PathBuf,
        output: PathBuf,
        /// cs5, cs5.5, cs6 or cc (AS1/2 SWFs need cs6 or older)
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        compressed: bool,
        /// Skip the .as files next to the document
        #[arg(long)]
        no_scripts: bool,
    },
    /// Convert a SWF to an Iggy file, skipping what Iggy cannot store (warnings go to stderr)
    #[command(after_help = "Examples:
  lumen-swf-cli swf2iggy movie.swf movie.iggy
  lumen-swf-cli swf2iggy movie.swf movie.iggy --variant 32")]
    Swf2iggy {
        input: PathBuf,
        output: PathBuf,
        /// 64, 32 or 32_legacy
        #[arg(long, default_value = "64")]
        variant: String,
    },
    /// Convert an Iggy file to a SWF
    #[command(after_help = "Examples:
  lumen-swf-cli iggy2swf movie.iggy movie.swf")]
    Iggy2swf { input: PathBuf, output: PathBuf },
    /// Print a shared object (.sol) as AMF text, or with --set store edited text in OUT
    #[command(after_help = "Examples:
  lumen-swf-cli sol data.sol
  lumen-swf-cli sol data.sol --set data.txt --out edited.sol")]
    Sol {
        file: PathBuf,
        #[arg(long, requires = "out")]
        set: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print one AMF3 value (such as ByteArray.writeObject output) as AMF text
    #[command(after_help = "Examples:
  lumen-swf-cli amf value.bin")]
    Amf { file: PathBuf },
    /// Rewrite every AS3 method with obfuscation traps removed
    #[command(after_help = "Examples:
  lumen-swf-cli deobfuscate in.swf out.swf
  lumen-swf-cli deobfuscate in.swf out.swf --dead-code
  lumen-swf-cli deobfuscate in.swf out.swf --group-parts")]
    Deobfuscate {
        input: PathBuf,
        output: PathBuf,
        /// Only remove unreachable code
        #[arg(long, conflicts_with = "group_parts")]
        dead_code: bool,
        /// Move blocks reached by a single jump to where they are jumped to
        #[arg(long)]
        group_parts: bool,
    },
    /// Write an uncompressed copy (also removes Harman encryption)
    #[command(after_help = "Examples:
  lumen-swf-cli decompress in.swf out.swf")]
    Decompress { input: PathBuf, output: PathBuf },
    /// Recompress with zlib, lzma or abc
    #[command(after_help = "Examples:
  lumen-swf-cli compress in.swf out.swf --format lzma
  lumen-swf-cli compress in.swf out.swf --format zlib --encrypt")]
    Compress {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value = "zlib")]
        format: String,
        /// Apply Harman AIR encryption
        #[arg(long)]
        encrypt: bool,
    },
}

/// mxmlc's command line: `-name=value`, `-name+=value`, or `-name` followed by its values, with the source to
/// compile last. Returns the options and the output path.
fn mxmlc_options(args: &[String]) -> Result<(lumen::asc::flex2::ApplicationOptions, PathBuf)> {
    let mut o = lumen::asc::flex2::ApplicationOptions::default();
    let mut output = None;
    let mut files = Vec::new();
    let mut i = 0;
    let flag = |v: &str| -> Result<bool> {
        match v {
            "" | "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(anyhow!("'{v}' is not a valid boolean")),
        }
    };
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        let Some(opt) = arg.strip_prefix('-') else {
            files.push(PathBuf::from(arg));
            continue;
        };
        let opt = opt.trim_start_matches('-');
        let (name, inline) = match opt.split_once("+=").or_else(|| opt.split_once('=')) {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (opt, None),
        };
        let name = name.strip_prefix("compiler.").unwrap_or(name);
        let mut take = |n: usize, list: bool| -> Vec<String> {
            if let Some(v) = &inline {
                return if list { v.split(',').filter(|s| !s.is_empty()).map(String::from).collect() } else { vec![v.clone()] };
            }
            let mut out = Vec::new();
            while i < args.len() && (list || out.len() < n) && !args[i].starts_with('-') {
                out.push(args[i].clone());
                i += 1;
            }
            out
        };
        match name {
            "source-path" | "sp" => o.source_path.extend(take(0, true).into_iter().map(PathBuf::from)),
            "library-path" | "l" => o.library_path.extend(take(0, true).into_iter().map(PathBuf::from)),
            "external-library-path" | "el" => o.external_library_path.extend(take(0, true).into_iter().map(PathBuf::from)),
            "includes" => o.includes.extend(take(0, true)),
            "output" | "o" => output = take(1, false).pop().map(PathBuf::from),
            "target-player" => o.target_player = take(1, false).pop().unwrap_or_default(),
            "swf-version" => o.swf_version = take(1, false).pop().unwrap_or_default().parse()?,
            "default-size" => {
                let v = take(2, false);
                let [w, h] = v.as_slice() else { return Err(anyhow!("-default-size takes a width and a height")) };
                o.default_size = (w.parse()?, h.parse()?);
            }
            "default-frame-rate" => o.default_frame_rate = take(1, false).pop().unwrap_or_default().parse()?,
            "default-background-color" => {
                let v = take(1, false).pop().unwrap_or_default();
                o.default_background_color = match v.strip_prefix("0x").or_else(|| v.strip_prefix('#')) {
                    Some(h) => i32::from_str_radix(h, 16)?,
                    None => v.parse()?,
                };
            }
            "define" => {
                let v = take(2, false);
                match v.as_slice() {
                    [n, val] => o.defines.push((n.clone(), val.clone())),
                    [nv] => {
                        let (n, val) = nv.split_once(',').ok_or_else(|| anyhow!("-define takes NAMESPACE::name,value"))?;
                        o.defines.push((n.to_string(), val.to_string()));
                    }
                    _ => return Err(anyhow!("-define takes NAMESPACE::name,value")),
                }
            }
            "debug" => o.debug = flag(&inline.unwrap_or_default())?,
            "strict" => o.strict = flag(&inline.unwrap_or_default())?,
            "optimize" => o.optimize = flag(&inline.unwrap_or_default())?,
            "omit-trace-statements" => o.omit_trace_statements = flag(&inline.unwrap_or_default())?,
            "warnings" => o.warnings = flag(&inline.unwrap_or_default())?,
            "compress" => o.compress = flag(&inline.unwrap_or_default())?,
            "static-link-runtime-shared-libraries" => {
                flag(&inline.unwrap_or_default())?;
            }
            "load-config" | "theme" | "fonts.local-fonts-snapshot" => {
                if inline.as_deref().is_some_and(|v| !v.is_empty()) {
                    return Err(anyhow!("-{name} with a file is not supported; pass the options directly"));
                }
            }
            _ => return Err(anyhow!("unknown configuration variable '{name}'")),
        }
    }
    let main = files.pop().ok_or_else(|| anyhow!("a source file to compile is required"))?;
    if !files.is_empty() {
        return Err(anyhow!("only one source file can be compiled"));
    }
    o.playerglobal = o.external_library_path.is_empty();
    let output = output.unwrap_or_else(|| main.with_extension("swf"));
    o.main = main;
    Ok((o, output))
}

fn open(path: &PathBuf) -> Result<Swf> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Swf::parse(&data)?)
}

fn abcs(swf: &Swf) -> Vec<(Vec<usize>, lumen::abc::Abc)> {
    let mut out = Vec::new();
    swf.walk(|path, t| {
        if let Some((data, _)) = lumen::abc::tag_abc(t) {
            match lumen::abc::Abc::parse(data) {
                Ok(abc) => out.push((path.to_vec(), abc)),
                Err(e) => eprintln!("tag {path:?}: {e}"),
            }
        }
    });
    out
}

fn as2_scripts(swf: &Swf) -> Vec<(String, lumen::action::scripts::Source)> {
    lumen::action::scripts::export_paths(&lumen::action::scripts::tree(swf))
}

fn print_tree(nodes: &[Node], depth: usize) {
    for n in nodes {
        let path = n.path.as_deref().map(|p| format!("  [{p}]")).unwrap_or_default();
        println!("{}{}{}", "  ".repeat(depth), n.label, path);
        print_tree(&n.children, depth + 1);
    }
}

fn tag_at<'a>(swf: &'a Swf, path: &str) -> Result<&'a lumen::Tag> {
    let p = tree::parse_path(path).ok_or_else(|| anyhow!("bad tag path '{path}'"))?;
    swf.tag(&p).ok_or_else(|| anyhow!("no tag at '{path}'"))
}

/// The AS3 method of `name` labelled `label`, with the ABC tag that holds it.
fn as3_method(swf: &Swf, name: &str, label: &str) -> Result<(Vec<usize>, lumen::abc::Abc, lumen::abc::scripts::MethodRef)> {
    use lumen::abc::scripts::{entries, methods};
    for (path, abc) in abcs(swf) {
        let Some(e) = entries(&abc).into_iter().find(|e| e.full_name() == name) else { continue };
        let all = methods(&abc, &e);
        return match all.iter().find(|m| m.label == label) {
            Some(m) => Ok((path, abc.clone(), m.clone())),
            None => Err(anyhow!("{name} has no method '{label}'; it has: {}", all.iter().map(|m| m.label.as_str()).collect::<Vec<_>>().join(", "))),
        };
    }
    Err(anyhow!("no AS3 script named '{name}'"))
}

fn save(swf: &Swf, out: &PathBuf) -> Result<()> {
    let opts = SaveOptions { compression: swf.header.compression, encrypt: swf.header.encrypted };
    std::fs::write(out, swf.save(&opts)?).with_context(|| format!("writing {}", out.display()))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    lumen::set_auto_deobfuscate(cli.deobfuscate);
    match cli.cmd {
        Cmd::Mxmlc { args } => {
            let (opts, output) = mxmlc_options(&args)?;
            let errors = match lumen::asc::flex2::compile_application(&opts) {
                Ok(app) => {
                    for d in &app.diagnostics {
                        eprintln!("{}", d.text);
                    }
                    std::fs::write(&output, &app.swf).with_context(|| format!("writing {}", output.display()))?;
                    let shown = std::fs::canonicalize(&output).unwrap_or(output.clone());
                    println!("{} ({} bytes)", shown.display().to_string().trim_start_matches(r"\\?\"), app.swf.len());
                    0
                }
                Err(diagnostics) => {
                    for d in &diagnostics {
                        eprintln!("{}", d.text);
                    }
                    diagnostics.iter().filter(|d| d.level == lumen::asc::flex2::Level::Error).count().max(1)
                }
            };
            if errors > 0 {
                std::process::exit(errors as i32);
            }
        }
        Cmd::Deobfuscate { input, output, dead_code, group_parts } => {
            use lumen::edit::Deobfuscation as D;
            let level = if dead_code {
                D::DeadCode
            } else if group_parts {
                D::GroupParts
            } else {
                D::Traps
            };
            let mut swf = open(&input)?;
            let n = lumen::edit::deobfuscate(&mut swf, level).map_err(|e| anyhow!(e))?;
            save(&swf, &output)?;
            println!("{n} methods deobfuscated");
        }
        Cmd::Info { file } => {
            let swf = open(&file)?;
            let h = &swf.header;
            println!("Signature:   {}", h.signature);
            println!("Compression: {:?}{}", h.compression, if h.encrypted { " (Harman encrypted)" } else { "" });
            println!("Version:     {}", h.version);
            println!("File size:   {}", h.file_size);
            println!("Frame size:  {} x {} px", swf.frame_rect.width_px(), swf.frame_rect.height_px());
            println!("Frame rate:  {}", swf.frame_rate);
            println!("Frames:      {}", swf.frame_count);
            println!("Tags:        {}", swf.timeline.tags.len());
            println!("ActionScript {}", if swf.is_as3() { "3" } else { "1/2" });
            for w in &swf.warnings {
                println!("Warning:     {w}");
            }
        }
        Cmd::Tags { file, raw } => {
            let swf = open(&file)?;
            print_tree(&if raw { tree::tag_list(&swf) } else { tree::resource_tree(&swf) }, 0);
        }
        Cmd::Tag { file, path } => {
            let swf = open(&file)?;
            for f in lumen::info::describe(tag_at(&swf, &path)?) {
                println!("{}: {}", f.name, f.value);
            }
        }
        Cmd::ExtractTag { file, path, out } => {
            let swf = open(&file)?;
            std::fs::write(&out, &tag_at(&swf, &path)?.body)?;
        }
        Cmd::Export { file, outdir, kinds, png, wav, shapes_png, frames_svg, texts_svg, zoom, pcode } => {
            let swf = open(&file)?;
            let kinds = if kinds.is_empty() {
                Kind::ALL.to_vec()
            } else {
                kinds.iter().map(|k| Kind::parse(k).ok_or_else(|| anyhow!("unknown kind '{k}'"))).collect::<Result<_>>()?
            };
            let opts = Options { png_only: png, wav_only: wav, shapes_png, frames_svg, texts_svg, zoom, scripts_pcode: pcode };
            let (written, errors) = Exporter::new(&swf).write_all(&outdir, &kinds, opts);
            println!("{} files written to {}", written.len(), outdir.display());
            for e in &errors {
                eprintln!("error: {e}");
            }
        }
        Cmd::Render { file, what, out, zoom } => {
            let swf = open(&file)?;
            let r = lumen::render::Renderer::new(&swf);
            let svg = match what.strip_prefix("frame:") {
                Some(n) => r.frame_svg(n.parse::<usize>()?.saturating_sub(1)),
                None => {
                    let id: u16 = what.parse()?;
                    match r.character(id) {
                        Some(t) if lumen::morph::is_morph(t.code) && out.extension().is_some_and(|e| e == "svg") => r.morph_svg(id)?,
                        _ => r.character_svg(id, 0, lumen::render::display::BUTTON_UP)?,
                    }
                }
            };
            if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                std::fs::write(&out, lumen::render::raster::png(&svg, zoom)?)?;
            } else {
                std::fs::write(&out, svg)?;
            }
        }
        Cmd::Frame { file, frame } => {
            let swf = open(&file)?;
            let list = lumen::render::display::display_list(&swf.timeline.tags, frame.saturating_sub(1));
            for p in list.values() {
                let m = &p.matrix;
                println!(
                    "depth {:>4}  char {:>5}  since {:>3}  ratio {:>5}  clip {:>4}  matrix [{:.3} {:.3} {:.3} {:.3} {} {}]{}",
                    p.depth, p.id, p.since + 1, p.ratio, p.clip_depth, m.a, m.b, m.c, m.d, m.tx, m.ty,
                    p.name.as_deref().map(|n| format!("  name {n}")).unwrap_or_default()
                );
            }
        }
        Cmd::Scripts { file } => {
            let swf = open(&file)?;
            for (_, abc) in abcs(&swf) {
                for e in lumen::abc::scripts::entries(&abc) {
                    println!("{:<10} {}", e.kind, e.full_name());
                }
            }
            for (name, _) in as2_scripts(&swf) {
                println!("{:<10} {name}", "as2");
            }
        }
        Cmd::Pcode { file, name, method: Some(label) } => {
            let swf = open(&file)?;
            let (_, abc, m) = as3_method(&swf, &name, &label)?;
            print!("{}", m.pcode(&abc));
        }
        Cmd::Pcode { file, name, method: None } => {
            let swf = open(&file)?;
            let mut found = false;
            for (_, abc) in abcs(&swf) {
                for e in lumen::abc::scripts::entries(&abc) {
                    if e.full_name() == name {
                        print!("{}", lumen::abc::scripts::entry_pcode(&abc, &e));
                        found = true;
                    }
                }
            }
            for (n, src) in as2_scripts(&swf) {
                if n == name {
                    print!("{}", lumen::action::pcode::to_pcode(&src.actions(&swf)));
                    found = true;
                }
            }
            if !found {
                return Err(anyhow!("no script named '{name}'"));
            }
        }
        Cmd::As3 { file, name } => {
            let swf = open(&file)?;
            let all = abcs(&swf);
            let refs: Vec<&lumen::abc::Abc> = all.iter().map(|(_, a)| a).collect();
            let index = lumen::abc::decomp::index_for(&refs);
            let ctx = lumen::abc::decomp::SwfContext::of(&swf);
            let mut found = false;
            for (_, abc) in &all {
                for e in lumen::abc::scripts::entries(abc) {
                    if e.full_name() == name {
                        print!("{}", lumen::abc::decomp::entry_source(&index, abc, &e, &ctx));
                        found = true;
                    }
                }
            }
            if !found {
                return Err(anyhow!("no script named '{name}'"));
            }
        }
        Cmd::As2 { file, name } => {
            let swf = open(&file)?;
            let src = as2_scripts(&swf).into_iter().find(|(n, _)| *n == name).map(|(_, s)| s).ok_or_else(|| anyhow!("no script named '{name}'"))?;
            print!("{}", Exporter::new(&swf).as2_source(&src));
        }
        Cmd::Replace { input, output, target, data, method, raw, chars } => {
            let mut swf = open(&input)?;
            let bytes = std::fs::read(&data).with_context(|| format!("reading {}", data.display()))?;
            let text = || String::from_utf8_lossy(&bytes).into_owned();
            if raw {
                let p = tree::parse_path(&target).ok_or_else(|| anyhow!("bad tag path '{target}'"))?;
                swf.set_tag_body(&p, bytes)?;
            } else if let Ok(id) = target.parse::<u16>() {
                let p = swf.find_character(id).ok_or_else(|| anyhow!("no character {id}"))?;
                match chars {
                    Some(c) => {
                        lumen::importers::font(&mut swf, &p, &bytes, &c.chars().collect::<Vec<_>>())?;
                    }
                    None => lumen::importers::import(&mut swf, &p, &bytes)?,
                }
            } else if let Some(label) = method {
                let (path, mut abc, m) = as3_method(&swf, &target, &label)?;
                lumen::abc::asm::assemble_method(&mut abc, m.method, m.owner, &text())?;
                swf.set_abc(&path, &abc)?;
            } else if let Some((_, src)) = as2_scripts(&swf).into_iter().find(|(n, _)| *n == target) {
                if data.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcode")) {
                    let list = lumen::action::asm::parse_pcode(&text(), swf.header.version)?;
                    src.set_actions(&mut swf, &list)?;
                } else {
                    use lumen::action::compile::{compile, swf_classes, Target};
                    use lumen::action::scripts::Kind;
                    let target = match src.kind {
                        Kind::ButtonCond => Target::ButtonCond,
                        Kind::ClipAction => Target::ClipAction,
                        _ => Target::Actions,
                    };
                    let c = compile(&text(), swf.header.version, target, &swf_classes(&swf), Some(&swf))?;
                    src.set_compiled(&mut swf, &c)?;
                }
            } else if let Some((path, id)) = abcs(&swf).iter().find_map(|(p, a)| lumen::abc::scripts::entries(a).into_iter().find(|e| e.full_name() == target).map(|e| (p.clone(), e.id()))) {
                let dir = data.parent().map(Path::to_path_buf).unwrap_or_default();
                let include = |name: &str| std::fs::read(dir.join(name)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
                lumen::edit::set_script_source_with(&mut swf, &path, &id, &text(), &include).map_err(|e| anyhow!("line {}: {}", e.line, e.message))?;
            } else {
                return Err(anyhow!("no character or script '{target}'"));
            }
            save(&swf, &output)?;
        }
        Cmd::Bundle { file, outdir } => {
            let data = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
            let b = lumen::bundle::open(&file, &data)?;
            for (key, bytes) in &b.entries {
                println!("{key}  {} bytes", bytes.len());
                if let Some(dir) = &outdir {
                    let rel: PathBuf = Path::new(key.trim_matches(['[', ']'])).components().filter(|c| matches!(c, std::path::Component::Normal(_))).collect();
                    let out = dir.join(rel);
                    let out = if out.extension().is_none() { out.with_extension("swf") } else { out };
                    if let Some(p) = out.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::write(&out, bytes).with_context(|| format!("writing {}", out.display()))?;
                }
            }
        }
        Cmd::Swc { input, output, no_deps } => {
            std::fs::write(&output, lumen::export::swc::swc(&open(&input)?, no_deps)?)?;
        }
        Cmd::Xfl { input, output, version, compressed, no_scripts } => {
            let swf = open(&input)?;
            let version = match version {
                Some(v) => lumen::xfl::FlaVersion::parse(&v).ok_or_else(|| anyhow!("unknown FLA version {v}"))?,
                None if swf.is_as3() => lumen::xfl::FlaVersion::Cc,
                None => lumen::xfl::FlaVersion::Cs6,
            };
            let mtime = std::fs::metadata(&input).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
            let opts = lumen::xfl::Options { compressed, version, export_script: !no_scripts, timestamp: mtime, ..Default::default() };
            let base = output.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "movie".into());
            let dir = output.parent().map(Path::to_path_buf).unwrap_or_default();
            let out = lumen::xfl::convert(&swf, &base, &opts)?;
            lumen::xfl::convert::write(&out, &dir, &base, compressed)?;
        }
        Cmd::Swf2iggy { input, output, variant } => {
            let variant = lumen::iggy::file::Variant::parse(&variant)?;
            let name = input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "movie.swf".into());
            let (mut file, warnings) = lumen::iggy::from_swf::create(&open(&input)?, variant, &name)?;
            for w in warnings {
                eprintln!("Warning: {w}");
            }
            std::fs::write(&output, file.save()?)?;
        }
        Cmd::Iggy2swf { input, output } => {
            let data = std::fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let name = input.file_name().map(|n| n.to_string_lossy().into_owned());
            let file = lumen::iggy::file::IggyFile::read(&data, name.as_deref())?;
            std::fs::write(&output, lumen::iggy::to_swf::swf_bytes(&file)?)?;
        }
        Cmd::Sol { file, set, out } => {
            let mut sol = lumen::amf::Sol::parse(&std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?)?;
            match (set, out) {
                (Some(text), Some(out)) => {
                    sol.set_text(&std::fs::read_to_string(&text)?).map_err(|e| anyhow!("{e}"))?;
                    std::fs::write(&out, sol.write()?)?;
                }
                _ => println!("{}", sol.to_text()),
            }
        }
        Cmd::Amf { file } => {
            let (amf, v) = lumen::amf::read_amf3(&std::fs::read(&file)?)?;
            println!("{}", lumen::amf::to_text(&amf, &v));
        }
        Cmd::Decompress { input, output } => {
            std::fs::write(&output, open(&input)?.uncompressed_bytes()?)?;
        }
        Cmd::Compress { input, output, format, encrypt } => {
            let compression = Compression::parse(&format).ok_or_else(|| anyhow!("unknown format '{format}'"))?;
            std::fs::write(&output, open(&input)?.save(&SaveOptions { compression, encrypt })?)?;
        }
    }
    Ok(())
}
