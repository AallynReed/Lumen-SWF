# Rust-FFDec

A Rust port of [JPEXS Free Flash Decompiler](https://github.com/jindrapetrik/jpexs-decompiler), shipped as a standalone Windows installer. No Java required.

> **Status: phase 6 of 7 done.** Opens, inspects and re-saves SWF files, exports their assets, renders shapes, sprites and frames, decompiles ActionScript 3 and ActionScript 1/2, and edits assets, texts, shapes, fonts, ActionScript and p-code. See [Roadmap](#roadmap).

## What works

- Opens `FWS`, `CWS` (zlib), `ZWS` (LZMA), `ABC` (non-standard LZMA) and Scaleform `GFX`/`CFX` files, including Harman AIR encrypted SWFs (`fWS`/`cWS`/`zWS`).
- Unmodified files re-save byte for byte. This is checked against all 332 SWFs in the JPEXS test corpus.
- Save, or save as uncompressed, zlib, LZMA or ABC, with or without Harman encryption. Edits stay in memory until saved, and the app asks before unsaved changes are lost.
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
- Editing, ported from FFDec's importers and assemblers:
  - replace images (DefineBits becomes DefineBitsJPEG2, as in FFDec), sounds (MP3, or WAV stored uncompressed), binary data and edit text;
  - edit static texts as FFDec's formatted text, or replace their plain records; matches FFDec 26.4 on all 153 corpus texts, and keeps letter spacing that FFDec's re-import loses;
  - replace shapes with an image or an SVG (paths, basic shapes, arcs, CSS, gradients, bitmap patterns), matching FFDec 26.4 on its SVG export of every corpus shape;
  - redraw a DefineFont2/3 from a TrueType or OpenType file and add characters to it, laying out the texts that use it again; DefineFont4 takes the file as its font data;
  - edit the SWF version, frame rate, stage size and background colour, and remove tags;
  - edit AS1/2 scripts as ActionScript, compiled by a port of FFDec's compiler that matches FFDec 26.4's bytes on all 192 corpus scripts;
  - edit AS3 scripts as ActionScript, compiled by a port of FFDec's AS3 compiler that matches FFDec 26.4's bytes on all 1,207 single-definition corpus scripts it compiles. Unlike FFDec, a class in a script that defines several things compiles without making the new script the SWF's entry point or leaving the old initializer creating the removed class, Flex embed classes keep their names, and code that would fail Flash Player's verifier is refused;
  - edit AS1/2 p-code per script and AS3 p-code per method, with errors reported by line. Every corpus script and all 12,229 AS3 methods re-assemble to themselves, and re-serialized ABCs match byte for byte;
  - an edited AS3 method's stack, scope and register limits grow to fit its code. FFDec keeps the old limits, so its p-code edits can fail Flash Player's verifier.
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
ffdec-cli pcode movie.swf com.example.Main --method "instance initializer"
ffdec-cli replace in.swf out.swf 4 image.png
ffdec-cli replace in.swf out.swf 7 shape.svg
ffdec-cli replace in.swf out.swf 2 font.ttf --chars "àéü€"
ffdec-cli replace in.swf out.swf frame_1/DoAction script.as
ffdec-cli replace in.swf out.swf frame_1/DoAction script.pcode
ffdec-cli replace in.swf out.swf com.example.Main Main.as
ffdec-cli replace in.swf out.swf com.example.Main method.pcode --method foo
ffdec-cli replace in.swf out.swf 12/3 body.bin --raw
```

A tag path is the tag's index in the root timeline, then its index inside each nested sprite. `replace` takes a character id, a script name, or with `--raw` a tag path. An AS1/2 script is assembled from `.pcode` and compiled from anything else; an AS3 class is compiled whole, or one method's p-code is assembled with `--method`.

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
6. **Editing:** asset, text, shape and font import, header editing, p-code editing and the AS1/2 and AS3 compilers. Done.
7. **Extras:** SWC/ZIP bundles, search, XFL/FLA export, Iggy, AMF.

## License

GPL-3.0-or-later. Rust-FFDec is a derivative of JPEXS Free Flash Decompiler by Jindra Petřík and contributors, licensed under GPLv3 (application) and LGPLv3 (library).
