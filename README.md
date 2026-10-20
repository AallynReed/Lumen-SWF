# Rust-FFDec

A Rust port of [JPEXS Free Flash Decompiler](https://github.com/jindrapetrik/jpexs-decompiler), shipped as a standalone Windows installer. No Java required.

> **Status: phase 1 of 7.** Opens, inspects and re-saves SWF files. Asset export, rendering and ActionScript decompilation are still to come; see [Roadmap](#roadmap).

## What works

- Opens `FWS`, `CWS` (zlib), `ZWS` (LZMA), `ABC` (non-standard LZMA) and Scaleform `GFX`/`CFX` files, including Harman AIR encrypted SWFs (`fWS`/`cWS`/`zWS`).
- Unmodified files re-save byte for byte. This is checked against all 332 SWFs in the JPEXS test corpus.
- Save a copy uncompressed, zlib, LZMA or ABC, with or without Harman encryption.
- FFDec-style resource tree (shapes, sprites, images, sounds, fonts, frames, scripts…) or the raw tag list, with filtering.
- Decoded fields for every common tag, plus a hex view of each tag body.
- Export any tag's raw body.
- Drag and drop, and the installer adds an "Open with" entry for `.swf`.

## Download

Grab the MSI from [Releases](https://github.com/AallynReed/Rust-FFDec/releases). Every push to `main` also builds one as a workflow artifact.

## Command line

`ffdec-cli` is in the release assets:

```
ffdec-cli info movie.swf
ffdec-cli tags movie.swf [--raw]
ffdec-cli tag movie.swf 12/3
ffdec-cli extract-tag movie.swf 12 body.bin
ffdec-cli decompress in.swf out.swf
ffdec-cli compress in.swf out.swf --format lzma [--encrypt]
```

A tag path is the tag's index in the root timeline, then its index inside each nested sprite.

## Building

Requires Rust (stable) and Node 20+.

```
npm ci
npx tauri build --bundles msi
```

The MSI ends up in `target/release/bundle/msi/`. For development, run `npx tauri dev`.

To run the corpus test, clone JPEXS next to the sources first:

```
git clone https://github.com/jindrapetrik/jpexs-decompiler.git
cargo test -p ffdec --release
```

## Layout

| Path | What |
| --- | --- |
| `crates/ffdec` | Core library: container, tags, tree, details |
| `crates/ffdec-cli` | Command-line tool |
| `src-tauri` | Desktop shell (Tauri 2) |
| `web` | UI |

## Roadmap

1. **Container and tags.** Done.
2. **Asset export:** images, sounds, binary data, fonts, texts, video.
3. **Rendering:** shapes, morph shapes, sprites and frames to SVG/PNG, with previews.
4. **ActionScript 3:** ABC bytecode, p-code, decompiler, deobfuscation.
5. **ActionScript 1/2:** actions, p-code, decompiler.
6. **Editing:** replacing assets, editing tags, recompiling scripts.
7. **Extras:** SWC/ZIP bundles, search, XFL/FLA export, Iggy, AMF.

## License

GPL-3.0-or-later. Rust-FFDec is a derivative of JPEXS Free Flash Decompiler by Jindra Petřík and contributors, licensed under GPLv3 (application) and LGPLv3 (library).
