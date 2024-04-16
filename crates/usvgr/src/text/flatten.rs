// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::mem;
use std::sync::Arc;

use fontdb::{Database, ID};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, RasterImageFormat};
use svgrtypes::AspectRatio;
use tiny_skia_path::{NonZeroRect, Transform};

use crate::*;

fn resolve_rendering_mode(text: &Text) -> ShapeRendering {
    match text.rendering_mode {
        TextRendering::OptimizeSpeed => ShapeRendering::CrispEdges,
        TextRendering::OptimizeLegibility => ShapeRendering::GeometricPrecision,
        TextRendering::GeometricPrecision => ShapeRendering::GeometricPrecision,
    }
}

fn push_outline_paths(
    span: &layout::Span,
    builder: &mut tiny_skia_path::PathBuilder,
    new_children: &mut Vec<Node>,
    rendering_mode: ShapeRendering,
) {
    let builder = mem::replace(builder, tiny_skia_path::PathBuilder::new());

    if let Some(path) = builder.finish().and_then(|p| {
        Path::new(
            String::new(),
            span.visibility,
            span.fill.clone(),
            span.stroke.clone(),
            span.paint_order,
            rendering_mode,
            Arc::new(p),
            Transform::default(),
            None, // static_hash - text paths are dynamic
        )
    }) {
        new_children.push(Node::Path(Box::new(path)));
    }
}

pub(crate) fn flatten(text: &mut Text, fontdb: &fontdb::Database) -> Option<(Group, NonZeroRect)> {
    let mut new_children = vec![];

    let rendering_mode = resolve_rendering_mode(text);

    for span in &text.layouted {
        if let Some(path) = span.overline.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }

        if let Some(path) = span.underline.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }

        // Instead of always processing each glyph separately, we always collect
        // as many outline glyphs as possible by pushing them into the span_builder
        // and only if we encounter a different glyph, or we reach the very end of the
        // span to we push the actual outline paths into new_children. This way, we don't need
        // to create a new path for every glyph if we have many consecutive glyphs
        // with just outlines (which is the most common case).
        let mut span_builder = tiny_skia_path::PathBuilder::new();

        for glyph in &span.positioned_glyphs {
            // The font face is parsed only once per glyph and the glyph is resolved
            // in the same order as upstream: COLR, SVG, bitmap and finally the outline.
            match fontdb.glyph(glyph.font, glyph.id) {
                // A COLRv0 glyph. Will return a vector of paths that make up the glyph description.
                // TODO: Don't use black for foreground color? But not sure whether to use fill or stroke
                // color.
                Some(ResolvedGlyph::Colr(layers)) => {
                    push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

                    let mut group = Group {
                        transform: glyph.colr_transform(),
                        ..Group::empty()
                    };

                    for path in layers {
                        // TODO: Probably need to update abs_transform of children?
                        group.children.push(Node::Path(Box::new(path)));
                    }
                    group.calculate_bounding_boxes();

                    new_children.push(Node::Group(Box::new(group)));
                }
                // An SVG glyph. Will return the usvgr tree containing the glyph descriptions.
                Some(ResolvedGlyph::Svg(tree)) => {
                    push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

                    let mut group = Group {
                        transform: glyph.svg_transform(),
                        ..Group::empty()
                    };
                    // TODO: Probably need to update abs_transform of children?
                    group.children.push(Node::Group(Box::new(tree.root)));
                    group.calculate_bounding_boxes();

                    new_children.push(Node::Group(Box::new(group)));
                }
                // A bitmap glyph.
                Some(ResolvedGlyph::Raster(img)) => {
                    push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

                    let transform = if img.is_sbix {
                        glyph.sbix_transform(
                            img.x as f32,
                            img.y as f32,
                            img.glyph_bbox.map(|bbox| bbox.x_min).unwrap_or(0) as f32,
                            img.glyph_bbox.map(|bbox| bbox.y_min).unwrap_or(0) as f32,
                            img.pixels_per_em as f32,
                            img.height,
                        )
                    } else {
                        glyph.cbdt_transform(
                            img.x as f32,
                            img.y as f32,
                            img.pixels_per_em as f32,
                            img.height,
                        )
                    };

                    let mut group = Group {
                        transform,
                        ..Group::empty()
                    };
                    group.children.push(Node::Image(Box::new(img.image)));
                    group.calculate_bounding_boxes();

                    new_children.push(Node::Group(Box::new(group)));
                }
                Some(ResolvedGlyph::Outline(outline)) => {
                    if let Some(outline) = outline.transform(glyph.outline_transform()) {
                        span_builder.push_path(&outline);
                    }
                }
                None => {}
            }
        }

        push_outline_paths(span, &mut span_builder, &mut new_children, rendering_mode);

        if let Some(path) = span.line_through.as_ref() {
            let mut path = path.clone();
            path.rendering_mode = rendering_mode;
            new_children.push(Node::Path(Box::new(path)));
        }
    }

    let mut group = Group {
        id: text.id.clone(),
        static_hash: text.static_hash,
        ..Group::empty()
    };

    for child in new_children {
        group.children.push(child);
    }

    group.calculate_bounding_boxes();
    let stroke_bbox = group.stroke_bounding_box().to_non_zero_rect()?;
    Some((group, stroke_bbox))
}

struct PathBuilder {
    builder: tiny_skia_path::PathBuilder,
}

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

pub(crate) trait DatabaseExt {
    fn glyph(&self, id: ID, glyph_id: GlyphId) -> Option<ResolvedGlyph>;
}

/// A glyph resolved from a font face.
pub(crate) enum ResolvedGlyph {
    /// A COLRv0 glyph made of colored layers.
    Colr(Vec<Path>),
    /// An SVG glyph.
    Svg(Tree),
    /// A bitmap (`sbix`/`CBDT`) glyph.
    Raster(BitmapImage),
    /// A plain outline glyph (`glyf`, `CFF`, `CFF2`).
    Outline(tiny_skia_path::Path),
}

pub(crate) struct BitmapImage {
    image: Image,
    x: i16,
    y: i16,
    pixels_per_em: u16,
    glyph_bbox: Option<ttf_parser::Rect>,
    is_sbix: bool,
    height: f32,
}

impl DatabaseExt for Database {
    #[inline(never)]
    fn glyph(&self, id: ID, glyph_id: GlyphId) -> Option<ResolvedGlyph> {
        self.with_face_data(id, |data, face_index| -> Option<ResolvedGlyph> {
            let font = ttf_parser::Face::parse(data, face_index).ok()?;
            let tables = font.tables();

            if tables.colr.is_some() {
                if let Some(paths) = colr(&font, glyph_id) {
                    return Some(ResolvedGlyph::Colr(paths));
                }
            }

            if tables.svg.is_some() {
                if let Some(tree) = svg(&font, glyph_id) {
                    return Some(ResolvedGlyph::Svg(tree));
                }
            }

            if tables.sbix.is_some()
                || tables.bdat.is_some()
                || tables.ebdt.is_some()
                || tables.cbdt.is_some()
            {
                if let Some(img) = raster(&font, glyph_id) {
                    return Some(ResolvedGlyph::Raster(img));
                }
            }

            outline(&font, glyph_id).map(ResolvedGlyph::Outline)
        })?
    }
}

fn outline(font: &ttf_parser::Face, glyph_id: GlyphId) -> Option<tiny_skia_path::Path> {
    let mut builder = PathBuilder {
        builder: tiny_skia_path::PathBuilder::new(),
    };

    font.outline_glyph(glyph_id, &mut builder)?;
    builder.builder.finish()
}

fn raster(font: &ttf_parser::Face, glyph_id: GlyphId) -> Option<BitmapImage> {
    let image = font.glyph_raster_image(glyph_id, u16::MAX)?;

    if image.format == RasterImageFormat::PNG {
        let data = decode_png_glyph(image.data)?;
        let rect = NonZeroRect::from_xywh(0.0, 0.0, data.width as f32, data.height as f32)?;

        let bitmap_image = BitmapImage {
            image: Image {
                id: String::new(),
                visibility: Visibility::Visible,
                view_box: ViewBox {
                    rect,
                    aspect: AspectRatio::default(),
                },
                rendering_mode: ImageRendering::OptimizeQuality,
                abs_transform: Transform::default(),
                abs_bounding_box: rect,
                // Used for hashing instead of the image data, so it must identify
                // the decoded bitmap.
                origin_href: data.id.clone(),
                kind: ImageKind::DATA(Arc::new(data)),
            },
            x: image.x,
            y: image.y,
            pixels_per_em: image.pixels_per_em,
            glyph_bbox: font.glyph_bounding_box(glyph_id),
            // ttf-parser always checks sbix first, so if this table exists, it was used.
            is_sbix: font.tables().sbix.is_some(),
            height: image.height as f32,
        };

        return Some(bitmap_image);
    }

    None
}

fn svg(font: &ttf_parser::Face, glyph_id: GlyphId) -> Option<Tree> {
    // TODO: Technically not 100% accurate because the SVG format in a OTF font
    // is actually a subset/superset of a normal SVG, but it seems to work fine
    // for Twitter Color Emoji, so might as well use what we already have.
    let image = font.glyph_svg_image(glyph_id)?;
    Tree::from_data(image.data, &Options::default(), &fontdb::Database::new()).ok()
}

fn colr(font: &ttf_parser::Face, glyph_id: GlyphId) -> Option<Vec<Path>> {
    let mut paths = vec![];
    let mut glyph_painter = GlyphPainter {
        face: font,
        paths: &mut paths,
        builder: PathBuilder {
            builder: tiny_skia_path::PathBuilder::new(),
        },
    };

    font.paint_color_glyph(glyph_id, 0, &mut glyph_painter)?;

    Some(paths)
}

struct GlyphPainter<'a> {
    face: &'a ttf_parser::Face<'a>,
    paths: &'a mut Vec<Path>,
    builder: PathBuilder,
}

impl ttf_parser::colr::Painter for GlyphPainter<'_> {
    fn outline(&mut self, glyph_id: ttf_parser::GlyphId) {
        let builder = &mut self.builder;
        match self.face.outline_glyph(glyph_id, builder) {
            Some(v) => v,
            None => return,
        };
    }

    fn paint_foreground(&mut self) {
        self.paint_color(ttf_parser::RgbaColor::new(0, 0, 0, 255));
    }

    fn paint_color(&mut self, color: ttf_parser::RgbaColor) {
        let builder = mem::replace(
            &mut self.builder,
            PathBuilder {
                builder: tiny_skia_path::PathBuilder::new(),
            },
        );

        if let Some(path) = builder.builder.finish().and_then(|p| {
            let fill = Fill {
                paint: Paint::Color(Color::new_rgb(color.red, color.green, color.blue)),
                opacity: Opacity::new(f32::from(color.alpha) / 255.0).unwrap(),
                rule: FillRule::NonZero,
                context_element: None,
            };

            Path::new(
                String::new(),
                Visibility::Visible,
                Some(fill),
                None,
                PaintOrder::FillAndStroke,
                ShapeRendering::GeometricPrecision,
                Arc::new(p),
                Transform::default(),
                None, // static_hash - glyph layers are dynamic
            )
        }) {
            self.paths.push(path)
        }
    }
}

/// Decodes a PNG bitmap glyph (`sbix`/`CBDT`) into premultiplied RGBA data.
///
/// The id is derived from the encoded bytes, so the render caches that key images by id
/// (or by the image node's `origin_href`) can tell different glyph bitmaps apart.
fn decode_png_glyph(data: &[u8]) -> Option<PreloadedImageData> {
    use std::hash::{Hash, Hasher};

    let mut decoder = png::Decoder::new(data);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        // `normalize_to_color8` expands palettes.
        png::ColorType::Indexed => return None,
    };

    if rgba.len() != info.width as usize * info.height as usize * 4 {
        return None;
    }

    let mut hasher = siphasher::sip::SipHasher13::new();
    data.hash(&mut hasher);

    Some(PreloadedImageData {
        data: std::borrow::Cow::Owned(PreloadedImageData::blend_rgba_slice(&rgba)),
        width: info.width,
        height: info.height,
        id: format!("font-glyph-bitmap:{:016x}", hasher.finish()),
    })
}
