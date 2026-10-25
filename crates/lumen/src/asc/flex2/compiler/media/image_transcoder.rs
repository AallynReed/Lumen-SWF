//! Port of `flex2.compiler.media.ImageTranscoder`: a bitmap tag, or a sprite around a shape filled with it for a
//! scaling grid or smoothing.

use super::abstract_transcoder::{self, new_results, resolve_source, DefineTagClass, SCALE9BOTTOM, SCALE9LEFT, SCALE9RIGHT, SCALE9TOP};
use super::movie_transcoder::define_scaling_grid;
use crate::asc::flex2::compiler::source::VirtualFile;
use crate::asc::flex2::compiler::transcoder::{self, Args, PathResolver, ResultsRef, TranscoderException, TranscodingResults, CORE_PACKAGE};
use crate::asc::flex2::flash::swf::tags::{codes, new_place_object, new_sprite, Body, DefineShape, Tag, TagRef};
use crate::asc::flex2::flash::swf::types::*;

pub const SMOOTHING: &str = "smoothing";

/// `ImageTranscoder.ImageInfo`.
pub struct ImageInfo {
    pub define_bits: TagRef,
    pub width: i32,
    pub height: i32,
}

pub fn is_supported_attribute(attr: &str) -> bool {
    attr == SCALE9BOTTOM || attr == SCALE9LEFT || attr == SCALE9RIGHT || attr == SCALE9TOP || attr == SMOOTHING
}

/// `Boolean.parseBoolean`.
pub fn parse_boolean(s: Option<&String>) -> bool {
    s.is_some_and(|s| s.eq_ignore_ascii_case("true"))
}

/// `doTranscode`, with the subclass's `getImage`.
pub fn do_transcode(
    define_tag: Option<DefineTagClass>,
    context: &PathResolver,
    args: &mut Args,
    class_name: &str,
    generate_source: bool,
    get_image: &dyn Fn(&VirtualFile, &Args) -> Result<ImageInfo, TranscoderException>,
) -> Result<ResultsRef, TranscoderException> {
    let results = new_results(TranscodingResults::with_source(resolve_source(context, args)?));
    let new_name = args.get(transcoder::NEWNAME).cloned();
    let source = results.borrow().asset_source.clone().unwrap();
    let info = get_image(&source, args)?;
    let tag = if args.contains_key(SCALE9LEFT) || args.contains_key(SCALE9RIGHT) || args.contains_key(SCALE9TOP) || args.contains_key(SCALE9BOTTOM) {
        if args.get(SCALE9LEFT).is_none() || args.get(SCALE9RIGHT).is_none() || args.get(SCALE9TOP).is_none() || args.get(SCALE9BOTTOM).is_none() {
            return Err(scaling_grid_exception());
        }
        build_sliced_sprite(new_name, &info, args)?
    } else if args.contains_key(SMOOTHING) && parse_boolean(args.get(SMOOTHING)) {
        build_smoothing_sprite(new_name, &info)
    } else {
        info.define_bits.clone()
    };
    results.borrow_mut().define_tag = Some(tag);
    if generate_source {
        let base_class = get_associated_class(define_tag, results.borrow().define_tag.as_ref());
        abstract_transcoder::generate_source(&results, base_class, class_name, args, &[])?;
    }
    Ok(results)
}

pub fn scaling_grid_exception() -> TranscoderException {
    TranscoderException::new("the scaling grid must have all four corners defined")
}

fn shape_with_fills(bounds: Rect) -> DefineShape {
    DefineShape {
        bounds,
        edge_bounds: Some(bounds),
        shape_with_style: ShapeWithStyle { shape_records: Some(Vec::new()), fillstyles: Some(Vec::new()), linestyles: Some(Vec::new()) },
        ..Default::default()
    }
}

fn twips_scale() -> Matrix {
    let mut m = Matrix::default();
    m.set_scale(20.0, 20.0);
    m
}

/// `generateSlicedShape(refBitmap, r, width, height)`.
fn generate_sliced_shape(ref_bitmap: &TagRef, r: Rect, width: i32, height: i32) -> TagRef {
    let (slt, srt, stt, sbt) = (r.x_min, r.x_max, r.y_min, r.y_max);
    let mut shape = shape_with_fills(Rect::new(0, width, 0, height));
    let tsm = twips_scale();
    let fills = shape.shape_with_style.fillstyles.as_mut().unwrap();
    for _ in 0..9 {
        fills.push(FillStyle::new(FILL_BITS | FILL_BITS_NOSMOOTH, tsm, ref_bitmap.clone()));
    }
    let (dxa, dxb, dxc) = (slt, srt - slt, width - srt);
    let (dya, dyb, dyc) = (stt, sbt - stt, height - sbt);
    let recs = shape.shape_with_style.shape_records.as_mut().unwrap();
    let mv = |recs: &mut Vec<ShapeRecord>, x: i32, y: i32| {
        let mut s = StyleChangeRecord::default();
        s.set_move(x, y);
        recs.push(ShapeRecord::StyleChange(s));
    };
    mv(recs, 0, dya);
    add_edges_with_fill(recs, &[(0, -dya), (dxa, 0)], 0, 1);
    add_edges_with_fill(recs, &[(dxb, 0)], 0, 2);
    add_edges_with_fill(recs, &[(dxc, 0), (0, dya)], 0, 3);
    add_edges_with_fill(recs, &[(0, dyb)], 0, 6);
    add_edges_with_fill(recs, &[(0, dyc), (-dxc, 0)], 0, 9);
    add_edges_with_fill(recs, &[(-dxb, 0)], 0, 8);
    add_edges_with_fill(recs, &[(-dxa, 0), (0, -dyc)], 0, 7);
    add_edges_with_fill(recs, &[(0, -dyb)], 0, 4);
    mv(recs, dxa, 0);
    add_edges_with_fill(recs, &[(0, dya)], 2, 1);
    add_edges_with_fill(recs, &[(0, dyb)], 5, 4);
    add_edges_with_fill(recs, &[(0, dyc)], 8, 7);
    mv(recs, dxa + dxb, 0);
    add_edges_with_fill(recs, &[(0, dya)], 3, 2);
    add_edges_with_fill(recs, &[(0, dyb)], 6, 5);
    add_edges_with_fill(recs, &[(0, dyc)], 9, 8);
    mv(recs, 0, dya);
    add_edges_with_fill(recs, &[(dxa, 0)], 1, 4);
    add_edges_with_fill(recs, &[(dxb, 0)], 2, 5);
    add_edges_with_fill(recs, &[(dxc, 0)], 3, 6);
    mv(recs, 0, dya + dyb);
    add_edges_with_fill(recs, &[(dxa, 0)], 4, 7);
    add_edges_with_fill(recs, &[(dxb, 0)], 5, 8);
    add_edges_with_fill(recs, &[(dxc, 0)], 6, 9);
    Tag::new_ref(codes::DEFINE_SHAPE4, Body::DefineShape(shape))
}

/// `generateSmoothingShape(imageInfo)`.
fn generate_smoothing_shape(info: &ImageInfo) -> TagRef {
    let (w, h) = (info.width * 20, info.height * 20);
    let mut shape = shape_with_fills(Rect::new(0, w, 0, h));
    shape.shape_with_style.fillstyles.as_mut().unwrap().push(FillStyle::new(FILL_BITS | FILL_BITS_CLIP, twips_scale(), info.define_bits.clone()));
    let recs = shape.shape_with_style.shape_records.as_mut().unwrap();
    let mut start = StyleChangeRecord::default();
    start.set_fill_style1(1);
    start.set_move(w, h);
    recs.push(ShapeRecord::StyleChange(start));
    recs.push(ShapeRecord::straight(-w, 0));
    recs.push(ShapeRecord::straight(0, -h));
    recs.push(ShapeRecord::straight(w, 0));
    recs.push(ShapeRecord::straight(0, h));
    Tag::new_ref(codes::DEFINE_SHAPE, Body::DefineShape(shape))
}

fn add_edges_with_fill(recs: &mut Vec<ShapeRecord>, coords: &[(i32, i32)], left: i32, right: i32) {
    let mut scr = StyleChangeRecord::default();
    if left != 0 || right != 0 {
        scr.set_fill_style0(left);
        scr.set_fill_style1(right);
    }
    recs.push(ShapeRecord::StyleChange(scr));
    for &(x, y) in coords {
        recs.push(ShapeRecord::straight(x, y));
    }
}

fn place_in_sprite(sprite: &TagRef, shape: TagRef) {
    let mut po = new_place_object(shape, 10);
    let mut tm = Matrix::default();
    tm.set_scale(1.0, 1.0);
    po.set_matrix(Some(tm));
    let po = Tag::new_ref(codes::PLACE_OBJECT2, Body::PlaceObject(po));
    sprite.borrow_mut().sprite_mut().unwrap().tag_list.push(po);
}

/// `buildSlicedSprite(name, imageInfo, args)`.
pub fn build_sliced_sprite(name: Option<String>, info: &ImageInfo, args: &Args) -> Result<TagRef, TranscoderException> {
    let sprite = new_sprite(name);
    define_scaling_grid(&sprite, args)?;
    let rect = {
        let s = sprite.borrow();
        let g = s.sprite().unwrap().scaling_grid.clone().unwrap();
        let r = match &g.borrow().body {
            Body::DefineScalingGrid(g) => g.rect,
            _ => Rect::default(),
        };
        r
    };
    let shape = generate_sliced_shape(&info.define_bits, rect, info.width * 20, info.height * 20);
    place_in_sprite(&sprite, shape);
    Ok(sprite)
}

/// `buildSmoothingSprite(name, imageInfo)`.
pub fn build_smoothing_sprite(name: Option<String>, info: &ImageInfo) -> TagRef {
    let sprite = new_sprite(name);
    place_in_sprite(&sprite, generate_smoothing_shape(info));
    sprite
}

/// `getAssociatedClass(tag)`: `BitmapAsset` for a bare bitmap.
pub fn get_associated_class(define_tag: Option<DefineTagClass>, tag: Option<&TagRef>) -> Option<String> {
    if tag.is_some_and(|t| matches!(t.borrow().body, Body::DefineBits(_))) {
        return Some(format!("{CORE_PACKAGE}.BitmapAsset"));
    }
    abstract_transcoder::associated_class(define_tag, tag)
}
