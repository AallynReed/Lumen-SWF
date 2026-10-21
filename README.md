# Rust-FFDec

A Rust port of [JPEXS Free Flash Decompiler](https://github.com/jindrapetrik/jpexs-decompiler), shipped as a standalone Windows installer. No Java required.

Opens, inspects, edits and re-saves SWF files: asset export, rendering, ActionScript 1/2 and 3 decompiling and compiling, p-code editing, SWC/ZIP bundles, XFL/FLA and SWC export, Iggy files and AMF/shared objects.

## Features

Everything is ported from FFDec and checked against FFDec 26.4 on the 332 SWFs of the JPEXS test corpus.

### Files

- Opens uncompressed, zlib, LZMA and `ABC` SWFs, Scaleform `GFX`/`CFX`, and Harman AIR encrypted SWFs.
- Saves with any of those compressions, with or without encryption. Unmodified files re-save byte for byte.
- Bundles: every SWF in a ZIP, every library in an SWC, and SWFs embedded in other files such as projector `.exe`s. Edited ZIP and SWC entries save back into the archive.
- Iggy files open as SWF and save back into the `.iggy`. External bitmaps are drawn from the `.raw.iggytex` file beside the movie.
- Shared objects (`.sol`) open, edit as FFDec's AMF text and save.
- Drag and drop, and an "Open with" entry for `.swf`.

### Browsing

- FFDec's resource tree or the raw tag list, with filtering.
- Decoded fields for every common tag, and a hex view of each tag body.
- Previews of shapes, sprites, buttons, texts, images and sounds, with a frame player.
- Search through ActionScript, p-code or texts, plain or by regular expression, across all open files.

### Export

- Images, sounds, fonts, texts, binary data and video in FFDec's formats, folder layout and file names.
- Shapes, morph shapes, sprites, buttons and frames to SVG or PNG, with FFDec's renderer: gradients, bitmap fills, strokes, masks, colour transforms, blend modes and the common filters.
- Scripts as ActionScript or p-code.
- The whole file as FLA or XFL (timelines, tweens, library and scripts), as SWC, or as Iggy (64-bit, 32-bit or 32-bit legacy).

### ActionScript

- AS3 and AS1/2 p-code from FFDec's path-following readers, so obfuscated code survives.
- AS3 and AS1/2 decompilers. They reproduce JPEXS's own test expectations (366 of 369 AS3 methods, all AS3 classes and all 84 AS1/2 cases).
- AS3 and AS1/2 compilers that match FFDec byte for byte on all 192 AS1/2 and 1,207 single-definition AS3 corpus scripts.
- P-code editing per AS1/2 script or AS3 method, with errors reported by line.

### Editing

- Replace images, sounds, binary data, edit texts and static texts.
- Replace shapes with an image or an SVG.
- Redraw fonts from a TrueType or OpenType file and add characters.
- Edit the SWF version, frame rate, stage size and background colour, and remove tags.
- Edits stay in memory until saved, and the app asks before unsaved changes are lost.

### Where it differs from FFDec

- Edited AS3 methods get stack, scope and register limits that fit their code, and AS3 code that would fail Flash Player's verifier is refused.
- Compiling one class of a multi-class AS3 script leaves the rest of the script working, and Flex embed classes keep their names.
- Re-imported static texts keep their letter spacing.
- SWC export really replaces the document class with its stub.
- AMF3 integral doubles stay doubles, duplicate keys are kept, and dates show in UTC.
- SWFs converted from Iggy are written as UTF-8 and without a duplicate End tag.

## Download

Grab the MSI from [Releases](https://github.com/AallynReed/Rust-FFDec/releases). Every push to `main` publishes a release.

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
ffdec-cli bundle game.exe outdir
ffdec-cli swc movie.swf movie.swc
ffdec-cli xfl movie.swf movie.fla --compressed [--version cs6]
ffdec-cli swf2iggy movie.swf movie.iggy [--variant 32]
ffdec-cli iggy2swf movie.iggy movie.swf
ffdec-cli sol data.sol
ffdec-cli sol data.sol --set data.txt --out edited.sol
ffdec-cli amf value.bin
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
| `crates/ffdec` | Core library: container, tags, tree, export, rendering, ActionScript, editing, XFL, Iggy, AMF |
| `crates/ffdec-cli` | Command-line tool |
| `src-tauri` | Desktop shell (Tauri 2) |
| `web` | UI |

## Not ported

- Bevel and gradient bevel/glow filters in the renderer.
- AS3 deobfuscation.
- FFDec's embedded Flash Player, debugger, Swf2Exe and uploader.

## License

GPL-3.0-or-later. Rust-FFDec is a derivative of JPEXS Free Flash Decompiler by Jindra Petřík and contributors, licensed under GPLv3 (application) and LGPLv3 (library).
