# Rust-FFDec

A Rust port of [JPEXS Free Flash Decompiler](https://github.com/jindrapetrik/jpexs-decompiler), shipped as a standalone Windows installer. No Java required.

> **Status: phase 6 of 7, in progress.** Opens, inspects and re-saves SWF files, exports their assets, renders shapes, sprites and frames, decompiles ActionScript 3 and ActionScript 1/2, and edits assets, tags and scripts. See [Roadmap](#roadmap).

## What works

- Opens `FWS`, `CWS` (zlib), `ZWS` (LZMA), `ABC` (non-standard LZMA) and Scaleform `GFX`/`CFX` files, including Harman AIR encrypted SWFs (`fWS`/`cWS`/`zWS`).
- Unmodified files re-save byte for byte. This is checked against all 332 SWFs in the JPEXS test corpus.
- Save a copy uncompressed, zlib, LZMA or ABC, with or without Harman encryption.
- FFDec-style resource tree (shapes, sprites, images, sounds, fonts, frames, scripts…) or the raw tag list, with filtering.
- Decoded fields for every common tag, plus a hex view of each tag body.
- Export any tag's raw body.
- Asset preview and export, matching FFDec's formats, folder layout and file names:
  - images: JPEG/PNG/GIF as stored, or PNG when there is a separate alpha channel or a lossless bitmap (or PNG for everything);
  - sounds: MP3 as stored, PCM/ADPCM streams as WAV, other codecs in FLV (or WAV for everything, decoding ADPCM, Nellymoser and MP3);
  - fonts as TrueType (DefineFont4 as OpenType), texts as plain text, binary data (embedded SWFs saved as `.swf`), video streams as FLV.
- Export a single asset, or every asset of the chosen types into a folder.
- Rendering to SVG and PNG, following FFDec's renderer:
  - shapes with solid, gradient (including focal and linear-RGB) and bitmap fills, and strokes with caps, joins and FFDec's 1 px minimum width;
  - morph shapes, as an animated SVG or start/end PNGs;
  - static text from embedded fonts, and dynamic text with system fonts;
  - sprites, buttons (all four states) and root frames, with nested sprite timing, clip masks, colour transforms, blend modes and the common filters (blur, drop shadow, glow, colour matrix, convolution).
- Previews of every shape, sprite, button and text, with a frame player for sprites, buttons and the root timeline.
- ActionScript 3: ABC bytecode parsed and listed by package and class, with FFDec-format p-code (FFDec's path-following disassembler, so obfuscated method bodies survive). The p-code matches FFDec's on 99.97% of the corpus's 10,580 method bodies.
- ActionScript 3 decompiler, a port of FFDec's: whole classes with imports, metadata, slot initializers and inner functions, typed with the player's own library. It reproduces 366 of JPEXS's 369 method expectations and all 21 class expectations. The preview switches between ActionScript and p-code; scripts export as `.as` (or `.pcode` with `--pcode`).
- ActionScript 1/2: frame, sprite, button, clip event and `__Packages` class scripts listed like FFDec, with FFDec-format p-code from its path-following reader.
- ActionScript 1/2 decompiler, a port of FFDec's: AS2 classes and interfaces rebuilt from `__Packages` scripts, with their undeclared fields. It reproduces all 84 of JPEXS's AS1/2 expectations, and its ActionScript and p-code match FFDec 26.4's on every one of the corpus's 192 AS1/2 scripts. Scripts export to `scripts/` as `.as` (or `.pcode` with `--pcode`).
- Drag and drop, and the installer adds an "Open with" entry for `.swf`.

## Download

Grab the MSI from [Releases](https://github.com/AallynReed/Rust-FFDec/releases). Every push to `main` publishes a release, versioned `0.<phase>.<n>`.

## Command line

`ffdec-cli` is in the release assets:

```
ffdec-cli info movie.swf
ffdec-cli tags movie.swf [--raw]
ffdec-cli tag movie.swf 12/3
ffdec-cli extract-tag movie.swf 12 body.bin
ffdec-cli export movie.swf outdir [--kinds shapes,sprites] [--png] [--wav] [--shapes-png] [--frames-svg] [--texts-svg] [--pcode] [--zoom 2]
ffdec-cli render movie.swf 12 shape.svg
ffdec-cli render movie.swf frame:5 frame.png --zoom 2
ffdec-cli frame movie.swf 5
ffdec-cli scripts movie.swf
ffdec-cli pcode movie.swf com.example.Main
ffdec-cli pcode movie.swf frame_1/DoAction
ffdec-cli as3 movie.swf com.example.Main
ffdec-cli as2 movie.swf frame_1/DoAction
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
| `crates/ffdec` | Core library: container, tags, tree, details, export, rendering |
| `crates/ffdec-cli` | Command-line tool |
| `src-tauri` | Desktop shell (Tauri 2) |
| `web` | UI |

## Roadmap

1. **Container and tags.** Done.
2. **Asset export:** images, sounds, binary data, fonts, texts, video. Done.
3. **Rendering:** shapes, morph shapes, sprites and frames to SVG/PNG, with previews. Done; bevel and gradient filters still to come.
4. **ActionScript 3:** ABC bytecode, p-code and decompiler done; deobfuscation next.
5. **ActionScript 1/2:** actions, p-code and decompiler. Done.
6. **Editing:** replacing assets, editing tags, recompiling scripts.
7. **Extras:** SWC/ZIP bundles, search, XFL/FLA export, Iggy, AMF.

## License

GPL-3.0-or-later. Rust-FFDec is a derivative of JPEXS Free Flash Decompiler by Jindra Petřík and contributors, licensed under GPLv3 (application) and LGPLv3 (library).
