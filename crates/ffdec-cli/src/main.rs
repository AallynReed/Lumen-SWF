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
    /// List ActionScript 3 scripts (classes and package-level definitions)
    Scripts { file: PathBuf },
    /// Print the p-code of an AS3 script by full name, e.g. com.example.Main
    Pcode { file: PathBuf, name: String },
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
        Cmd::Export { file, outdir, kinds, png, wav, shapes_png, frames_svg, texts_svg, zoom } => {
            let swf = open(&file)?;
            let kinds = if kinds.is_empty() {
                Kind::ALL.to_vec()
            } else {
                kinds.iter().map(|k| Kind::parse(k).ok_or_else(|| anyhow!("unknown kind '{k}'"))).collect::<Result<_>>()?
            };
            let opts = Options { png_only: png, wav_only: wav, shapes_png, frames_svg, texts_svg, zoom };
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
            if !found {
                return Err(anyhow!("no script named '{name}'"));
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
