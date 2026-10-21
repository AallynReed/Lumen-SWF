use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use ffdec::export::{Exporter, Kind, Options};
use ffdec::tree::{self, Node};
use ffdec::{Compression, SaveOptions, Swf};

#[derive(Parser)]
#[command(name = "ffdec-cli", version, about = "Rust-FFDec command line")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the header and any parse warnings
    Info { file: PathBuf },
    /// Print the resource tree, or the raw tag list with --raw
    Tags {
        file: PathBuf,
        #[arg(long)]
        raw: bool,
    },
    /// Print the fields of one tag, addressed by path such as 12 or 12/3
    Tag { file: PathBuf, path: String },
    /// Write the raw body of one tag to a file
    ExtractTag { file: PathBuf, path: String, out: PathBuf },
    /// Export assets into <outdir>/<kind> folders, laid out like FFDec
    Export {
        file: PathBuf,
        outdir: PathBuf,
        /// Comma-separated kinds: images, sounds, binaryData, fonts, texts, movies, shapes,
        /// morphshapes, frames, sprites, buttons (default all)
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
    Render {
        file: PathBuf,
        what: String,
        out: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        zoom: f32,
    },
    /// Print the display list of a root frame (1-based)
    Frame { file: PathBuf, frame: usize },
    /// List AS3 scripts (classes and package-level definitions) and AS1/2 scripts (export paths)
    Scripts { file: PathBuf },
    /// Print the p-code of a script: an AS3 full name such as com.example.Main, or an AS1/2
    /// export path such as frame_1/DoAction
    Pcode {
        file: PathBuf,
        name: String,
        /// One AS3 method, such as "instance initializer", "foo" or "static get bar"
        #[arg(long)]
        method: Option<String>,
    },
    /// Print the decompiled ActionScript of an AS3 script by full name
    As3 { file: PathBuf, name: String },
    /// Print the decompiled ActionScript of an AS1/2 script by export path, e.g. frame_1/DoAction
    As2 { file: PathBuf, name: String },
    /// Replace a character (by id) with a file, or a script (by name) with ActionScript or
    /// .pcode, like FFDec's -replace
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
    Bundle { file: PathBuf, outdir: Option<PathBuf> },
    /// Write an uncompressed copy (also removes Harman encryption)
    Decompress { input: PathBuf, output: PathBuf },
    /// Recompress with zlib, lzma or abc
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

fn open(path: &PathBuf) -> Result<Swf> {
    let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Swf::parse(&data)?)
}

fn abcs(swf: &Swf) -> Vec<(Vec<usize>, ffdec::abc::Abc)> {
    let mut out = Vec::new();
    swf.walk(|path, t| {
        if let Some((data, _)) = ffdec::abc::tag_abc(t) {
            match ffdec::abc::Abc::parse(data) {
                Ok(abc) => out.push((path.to_vec(), abc)),
                Err(e) => eprintln!("tag {path:?}: {e}"),
            }
        }
    });
    out
}

fn as2_scripts(swf: &Swf) -> Vec<(String, ffdec::action::scripts::Source)> {
    ffdec::action::scripts::export_paths(&ffdec::action::scripts::tree(swf))
}

fn print_tree(nodes: &[Node], depth: usize) {
    for n in nodes {
        let path = n.path.as_deref().map(|p| format!("  [{p}]")).unwrap_or_default();
        println!("{}{}{}", "  ".repeat(depth), n.label, path);
        print_tree(&n.children, depth + 1);
    }
}

fn tag_at<'a>(swf: &'a Swf, path: &str) -> Result<&'a ffdec::Tag> {
    let p = tree::parse_path(path).ok_or_else(|| anyhow!("bad tag path '{path}'"))?;
    swf.tag(&p).ok_or_else(|| anyhow!("no tag at '{path}'"))
}

/// The AS3 method of `name` labelled `label`, with the ABC tag that holds it.
fn as3_method(swf: &Swf, name: &str, label: &str) -> Result<(Vec<usize>, ffdec::abc::Abc, ffdec::abc::scripts::MethodRef)> {
    use ffdec::abc::scripts::{entries, methods};
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
    match Cli::parse().cmd {
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
            for f in ffdec::info::describe(tag_at(&swf, &path)?) {
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
            let r = ffdec::render::Renderer::new(&swf);
            let svg = match what.strip_prefix("frame:") {
                Some(n) => r.frame_svg(n.parse::<usize>()?.saturating_sub(1)),
                None => {
                    let id: u16 = what.parse()?;
                    match r.character(id) {
                        Some(t) if ffdec::morph::is_morph(t.code) && out.extension().is_some_and(|e| e == "svg") => r.morph_svg(id)?,
                        _ => r.character_svg(id, 0, ffdec::render::display::BUTTON_UP)?,
                    }
                }
            };
            if out.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
                std::fs::write(&out, ffdec::render::raster::png(&svg, zoom)?)?;
            } else {
                std::fs::write(&out, svg)?;
            }
        }
        Cmd::Frame { file, frame } => {
            let swf = open(&file)?;
            let list = ffdec::render::display::display_list(&swf.timeline.tags, frame.saturating_sub(1));
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
                for e in ffdec::abc::scripts::entries(&abc) {
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
                for e in ffdec::abc::scripts::entries(&abc) {
                    if e.full_name() == name {
                        print!("{}", ffdec::abc::scripts::entry_pcode(&abc, &e));
                        found = true;
                    }
                }
            }
            for (n, src) in as2_scripts(&swf) {
                if n == name {
                    print!("{}", ffdec::action::pcode::to_pcode(&src.actions(&swf)));
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
            let refs: Vec<&ffdec::abc::Abc> = all.iter().map(|(_, a)| a).collect();
            let index = ffdec::abc::decomp::index_for(&refs);
            let ctx = ffdec::abc::decomp::SwfContext::of(&swf);
            let mut found = false;
            for (_, abc) in &all {
                for e in ffdec::abc::scripts::entries(abc) {
                    if e.full_name() == name {
                        print!("{}", ffdec::abc::decomp::entry_source(&index, abc, &e, &ctx));
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
                        ffdec::importers::font(&mut swf, &p, &bytes, &c.chars().collect::<Vec<_>>())?;
                    }
                    None => ffdec::importers::import(&mut swf, &p, &bytes)?,
                }
            } else if let Some(label) = method {
                let (path, mut abc, m) = as3_method(&swf, &target, &label)?;
                ffdec::abc::asm::assemble_method(&mut abc, m.method, m.owner, &text())?;
                swf.set_abc(&path, &abc)?;
            } else if let Some((_, src)) = as2_scripts(&swf).into_iter().find(|(n, _)| *n == target) {
                if data.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcode")) {
                    let list = ffdec::action::asm::parse_pcode(&text(), swf.header.version)?;
                    src.set_actions(&mut swf, &list)?;
                } else {
                    use ffdec::action::compile::{compile, swf_classes, Target};
                    use ffdec::action::scripts::Kind;
                    let target = match src.kind {
                        Kind::ButtonCond => Target::ButtonCond,
                        Kind::ClipAction => Target::ClipAction,
                        _ => Target::Actions,
                    };
                    let c = compile(&text(), swf.header.version, target, &swf_classes(&swf), Some(&swf))?;
                    src.set_compiled(&mut swf, &c)?;
                }
            } else if let Some((path, id)) = abcs(&swf).iter().find_map(|(p, a)| ffdec::abc::scripts::entries(a).into_iter().find(|e| e.full_name() == target).map(|e| (p.clone(), e.id()))) {
                ffdec::edit::set_script_source(&mut swf, &path, &id, &text()).map_err(|e| anyhow!("line {}: {}", e.line, e.message))?;
            } else {
                return Err(anyhow!("no character or script '{target}'"));
            }
            save(&swf, &output)?;
        }
        Cmd::Bundle { file, outdir } => {
            let data = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
            let b = ffdec::bundle::open(&file, &data)?;
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
