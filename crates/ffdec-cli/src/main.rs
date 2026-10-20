use std::path::PathBuf;

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
    Pcode { file: PathBuf, name: String },
    /// Print the decompiled ActionScript of an AS3 script by full name
    As3 { file: PathBuf, name: String },
    /// Print the decompiled ActionScript of an AS1/2 script by export path, e.g. frame_1/DoAction
    As2 { file: PathBuf, name: String },
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
    /// Replace the raw body of one tag
    ReplaceTag {
        file: PathBuf,
        path: String,
        body: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Replace an image asset (target can be a tag path like 12 or character ID like 4 or id:4)
    ReplaceImage {
        file: PathBuf,
        target: String,
        image: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Replace a sound asset (MP3, WAV, or raw sound data)
    ReplaceSound {
        file: PathBuf,
        target: String,
        sound: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Replace a binary data asset
    ReplaceBinary {
        file: PathBuf,
        target: String,
        binary: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Replace a script (AS1/2 p-code or AS3 bytecode/p-code)
    ReplaceScript {
        file: PathBuf,
        name: String,
        script: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
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

fn resolve_target_path(swf: &Swf, target: &str) -> Result<Vec<usize>> {
    if let Some(id_str) = target.strip_prefix("id:").or_else(|| target.strip_prefix("ID:")) {
        let id: u16 = id_str.parse().context("parsing character ID")?;
        return swf.find_character(id).ok_or_else(|| anyhow!("character ID {id} not found"));
    }
    if let Some(p) = tree::parse_path(target) {
        if swf.tag(&p).is_some() {
            return Ok(p);
        }
    }
    if let Ok(id) = target.parse::<u16>() {
        if let Some(p) = swf.find_character(id) {
            return Ok(p);
        }
    }
    Err(anyhow!("could not resolve target '{target}' to a tag path or character ID"))
}

fn save_swf(swf: &Swf, default_path: &PathBuf, out: Option<PathBuf>) -> Result<()> {
    let target = out.as_ref().unwrap_or(default_path);
    let opts = SaveOptions {
        compression: swf.header.compression,
        encrypt: swf.header.encrypted,
    };
    let bytes = swf.save(&opts)?;
    std::fs::write(target, bytes)?;
    println!("Saved to {}", target.display());
    Ok(())
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
        Cmd::Pcode { file, name } => {
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
        Cmd::Decompress { input, output } => {
            std::fs::write(&output, open(&input)?.uncompressed_bytes()?)?;
        }
        Cmd::Compress { input, output, format, encrypt } => {
            let compression = Compression::parse(&format).ok_or_else(|| anyhow!("unknown format '{format}'"))?;
            std::fs::write(&output, open(&input)?.save(&SaveOptions { compression, encrypt })?)?;
        }
        Cmd::ReplaceTag { file, path, body, out } => {
            let mut swf = open(&file)?;
            let p = tree::parse_path(&path).ok_or_else(|| anyhow!("bad tag path '{path}'"))?;
            let body_bytes = std::fs::read(&body).with_context(|| format!("reading {}", body.display()))?;
            swf.replace_tag_body(&p, &body_bytes)?;
            save_swf(&swf, &file, out)?;
        }
        Cmd::ReplaceImage { file, target, image, out } => {
            let mut swf = open(&file)?;
            let p = resolve_target_path(&swf, &target)?;
            let img_bytes = std::fs::read(&image).with_context(|| format!("reading {}", image.display()))?;
            ffdec::replace::replace_image(&mut swf, &p, &img_bytes)?;
            save_swf(&swf, &file, out)?;
        }
        Cmd::ReplaceSound { file, target, sound, out } => {
            let mut swf = open(&file)?;
            let p = resolve_target_path(&swf, &target)?;
            let snd_bytes = std::fs::read(&sound).with_context(|| format!("reading {}", sound.display()))?;
            ffdec::replace::replace_sound(&mut swf, &p, &snd_bytes)?;
            save_swf(&swf, &file, out)?;
        }
        Cmd::ReplaceBinary { file, target, binary, out } => {
            let mut swf = open(&file)?;
            let p = resolve_target_path(&swf, &target)?;
            let bin_bytes = std::fs::read(&binary).with_context(|| format!("reading {}", binary.display()))?;
            ffdec::replace::replace_binary(&mut swf, &p, &bin_bytes)?;
            save_swf(&swf, &file, out)?;
        }
        Cmd::ReplaceScript { file, name, script, out } => {
            let mut swf = open(&file)?;
            let script_data = std::fs::read(&script).with_context(|| format!("reading {}", script.display()))?;
            let mut found = false;

            // Check AS1/2 scripts
            for (n, src) in as2_scripts(&swf) {
                if n == name || n.trim_start_matches("scripts/") == name.trim_start_matches("scripts/") {
                    let script_text = String::from_utf8_lossy(&script_data);
                    let list = ffdec::action::asm::parse_pcode(&script_text, swf.header.version)?;
                    swf.replace_as2_script(&src.tag, &list)?;
                    found = true;
                    break;
                }
            }

            // Check AS3 scripts if not found
            if !found {
                for (p, abc) in abcs(&swf) {
                    // If script is a raw .abc binary
                    if script.extension().is_some_and(|e| e.eq_ignore_ascii_case("abc")) {
                        swf.replace_as3_script(&p, &script_data)?;
                        found = true;
                        break;
                    }
                    for e in ffdec::abc::scripts::entries(&abc) {
                        if e.full_name() == name {
                            let script_text = String::from_utf8_lossy(&script_data);
                            let code = ffdec::abc::asm::assemble_instructions(&script_text, &abc)?;
                            let mut new_abc = abc.clone();
                            if let Some(trait_idx) = e.trait_index {
                                let tr = &new_abc.scripts[e.script].traits[trait_idx];
                                let method_opt = match &tr.data {
                                    ffdec::abc::TraitData::Function { method, .. }
                                    | ffdec::abc::TraitData::Method { method, .. }
                                    | ffdec::abc::TraitData::Getter { method, .. }
                                    | ffdec::abc::TraitData::Setter { method, .. } => Some(*method),
                                    ffdec::abc::TraitData::Class { class, .. } => Some(new_abc.instances[*class].iinit),
                                    _ => None,
                                };
                                if let Some(m) = method_opt {
                                    if let Some(b_idx) = new_abc.body_of.get(m).copied().flatten() {
                                        new_abc.bodies[b_idx].code = code;
                                        swf.replace_as3_script(&p, &new_abc.to_bytes())?;
                                        found = true;
                                        break;
                                    }
                                }
                            } else {
                                let init = new_abc.scripts[e.script].init;
                                if let Some(b_idx) = new_abc.body_of.get(init).copied().flatten() {
                                    new_abc.bodies[b_idx].code = code;
                                    swf.replace_as3_script(&p, &new_abc.to_bytes())?;
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                    if found {
                        break;
                    }
                }
            }

            if !found {
                return Err(anyhow!("no script named '{name}'"));
            }
            save_swf(&swf, &file, out)?;
        }
    }
    Ok(())
}
