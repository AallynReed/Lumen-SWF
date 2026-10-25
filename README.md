# Lumen-SWF

A Flash SWF decompiler and editor written in Rust, shipped as a standalone Windows installer. No Java required.

Opens, inspects, edits and re-saves SWF files: asset export, rendering, ActionScript 1/2 and 3 decompiling and compiling, p-code editing, SWC/ZIP bundles, XFL/FLA and SWC export, Iggy files and AMF/shared objects.

## Features

Every feature is regression-tested on a corpus of 332 SWFs.

### Files

- Opens uncompressed, zlib, LZMA and `ABC` SWFs, Scaleform `GFX`/`CFX`, and Harman AIR encrypted SWFs.
- Saves with any of those compressions, with or without encryption. Unmodified files re-save byte for byte.
- Bundles: every SWF in a ZIP, every library in an SWC, and SWFs embedded in other files such as projector `.exe`s. Edited ZIP and SWC entries save back into the archive.
- Iggy files open as SWF and save back into the `.iggy`. External bitmaps are drawn from the `.raw.iggytex` file beside the movie.
- Shared objects (`.sol`) open, edit as AMF text and save.
- Drag and drop, and an "Open with" entry for `.swf`.

### Browsing

- A resource tree or the raw tag list, with filtering.
- Decoded fields for every common tag, and a hex view of each tag body.
- Previews of shapes, sprites, buttons, texts, images and sounds, with a frame player.
- Play the whole file, edits included, in the bundled [Ruffle](https://ruffle.rs) player.
- Search through ActionScript, p-code or texts, plain or by regular expression, across all open files.

### Export

- Images, sounds, fonts, texts, binary data and video.
- Shapes, morph shapes, sprites, buttons and frames to SVG or PNG: gradients, bitmap fills, strokes, masks, colour transforms, blend modes and the common filters.
- Scripts as ActionScript or p-code.
- The whole file as FLA or XFL (timelines, tweens, library and scripts), as SWC, or as Iggy (64-bit, 32-bit or 32-bit legacy).

### ActionScript

- AS3 and AS1/2 p-code from path-following readers, so obfuscated code survives.
- AS3 and AS1/2 decompilers, passing all 369 AS3 method cases, every AS3 class case and all 84 AS1/2 cases of the reference test suite. Decompiled AS3 compiles back with mxmlc to the same code.
- An AS3 compiler ported from Adobe's mxmlc (Flex SDK 4.8), byte-identical to mxmlc.jar on 53 real projects, used to compile edited AS3 classes. CSS and `[Embed]` of images, sounds, SWF symbols and data are ported too; MXML, font and SVG embedding, and data binding are not yet.
- An AS1/2 compiler, verified byte for byte on 192 corpus scripts.
- P-code editing per AS1/2 script or AS3 method, with errors reported by line.
- Automatic deobfuscation for AS1/2 and AS3, and permanent AS3 rewrites: remove traps, remove dead code, and GroupParts, which moves blocks reached by a single jump to where they are jumped to.

### Editing

- Replace images, sounds, binary data, edit texts and static texts.
- Replace shapes with an image or an SVG.
- Redraw fonts from a TrueType or OpenType file and add characters.
- Edit the SWF version, frame rate, stage size and background colour, and remove tags.
- Edits stay in memory until saved, and the app asks before unsaved changes are lost.
- Edited AS3 methods get stack, scope and register limits that fit their code, and AS3 code that would fail Flash Player's verifier is refused.
- Compiling one class of a multi-class AS3 script leaves the rest of the script working, and Flex embed classes keep their names.
- Re-imported static texts keep their letter spacing.
- SWC export replaces the document class with its stub.
- AMF3 integral doubles stay doubles, duplicate keys are kept, and dates show in UTC.
- SWFs converted from Iggy are written as UTF-8 and without a duplicate End tag.

## Download

Grab the MSI from [Releases](https://github.com/AallynReed/Lumen-SWF/releases). Every push to `main` publishes a release.

## Command line

`lumen-swf-cli` is in the release assets:

```
lumen-swf-cli info movie.swf
lumen-swf-cli tags movie.swf [--raw]
lumen-swf-cli tag movie.swf 12
lumen-swf-cli tag movie.swf 12/3
lumen-swf-cli extract-tag movie.swf 12 body.bin
lumen-swf-cli export movie.swf outdir [--kinds shapes,sprites] [--png] [--wav] [--shapes-png] [--frames-svg] [--texts-svg] [--pcode] [--zoom 2]
lumen-swf-cli export movie.swf outdir --kinds scripts --pcode
lumen-swf-cli export movie.swf outdir --kinds frames --frames-svg --zoom 2
lumen-swf-cli render movie.swf 12 shape.svg
lumen-swf-cli render movie.swf frame:5 frame.png --zoom 2
lumen-swf-cli frame movie.swf 5
lumen-swf-cli scripts movie.swf
lumen-swf-cli pcode movie.swf com.example.Main
lumen-swf-cli pcode movie.swf com.example.Main --method "instance initializer"
lumen-swf-cli pcode movie.swf frame_1/DoAction
lumen-swf-cli as3 movie.swf com.example.Main [--deobfuscate]
lumen-swf-cli as2 movie.swf frame_1/DoAction
lumen-swf-cli replace in.swf out.swf 4 image.png
lumen-swf-cli replace in.swf out.swf 7 shape.svg
lumen-swf-cli replace in.swf out.swf 2 font.ttf --chars "àéü€"
lumen-swf-cli replace in.swf out.swf frame_1/DoAction script.as
lumen-swf-cli replace in.swf out.swf frame_1/DoAction script.pcode
lumen-swf-cli replace in.swf out.swf com.example.Main Main.as
lumen-swf-cli replace in.swf out.swf com.example.Main method.pcode --method foo
lumen-swf-cli replace in.swf out.swf 12/3 body.bin --raw
lumen-swf-cli bundle game.exe [outdir]
lumen-swf-cli mxmlc -load-config= -external-library-path=playerglobal.swc -target-player=11.2 -swf-version=15 -default-size 601 502 -default-frame-rate 24 -source-path src lib -output out.swf src/Main.as
lumen-swf-cli swc movie.swf movie.swc [--no-deps]
lumen-swf-cli xfl movie.swf movie.fla [--compressed] [--version cs6]
lumen-swf-cli swf2iggy movie.swf movie.iggy [--variant 32]
lumen-swf-cli iggy2swf movie.iggy movie.swf
lumen-swf-cli sol data.sol
lumen-swf-cli sol data.sol --set data.txt --out edited.sol
lumen-swf-cli amf value.bin
lumen-swf-cli deobfuscate in.swf out.swf [--dead-code | --group-parts]
lumen-swf-cli decompress in.swf out.swf
lumen-swf-cli compress in.swf out.swf --format lzma [--encrypt]
lumen-swf-cli compress in.swf out.swf --format zlib --encrypt
```

`mxmlc` takes mxmlc.jar's own arguments and builds the same SWF, with mxmlc's error output and exit code. A tag path is the tag's index in the root timeline, then its index inside each nested sprite. `replace` takes a character id, a script name, or with `--raw` a tag path. An AS1/2 script is assembled from `.pcode` and compiled from anything else; an AS3 class is compiled whole, or one method's p-code is assembled with `--method`.

## Building

Requires Rust (stable) and Node 20+.

```
npm ci
npx tauri build --bundles msi
```

The MSI ends up in `target/release/bundle/msi/`. For development, run `npx tauri dev`.

Tests run locally before each push; CI only builds. The corpus tests read the test corpus from `corpus/` and skip when it is missing:

```
cargo test -p lumen --release
```

## Layout

| Path | What |
| --- | --- |
| `crates/lumen` | Core library: container, tags, tree, export, rendering, ActionScript, editing, XFL, Iggy, AMF |
| `crates/lumen-cli` | Command-line tool |
| `src-tauri` | Desktop shell (Tauri 2) |
| `web` | UI |

## License

GPL-3.0-or-later. Third-party notices are in [NOTICE](NOTICE).
