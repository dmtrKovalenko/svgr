// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;
use std::mem;
use std::sync::Arc;

use fontdb::{Database, ID};
use rustybuzz::ttf_parser;
use rustybuzz::ttf_parser::{GlyphId, RasterImageFormat, RgbaColor};
use svgrtypes::AspectRatio;
use tiny_skia_path::{NonZeroRect, Transform};
use xmlwriter::XmlWriter;

use crate::parser::OptionLog;
use crate::text::colr::GlyphPainter;
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
    abs_transform: Transform,
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
            abs_transform,
            None, // static_hash - text paths are dynamic
        )
    }) {
        new_children.push(Node::Path(Box::new(path)));
    }
}

/// A per-conversion cache of resolved glyphs.
///
/// Glyph lookups parse the font face and build outlines (or COLR/SVG/bitmap
/// glyphs), so a glyph used many times in a document is resolved only once.
/// It is keyed by font database IDs, so it must not outlive a single
/// conversion with a single font database: it is cleared by
/// [`Cache::clear`](crate::Cache::clear), which runs after every tree conversion.
#[derive(Default)]
pub(crate) struct GlyphCache {
    glyphs: HashMap<(ID, GlyphId), Option<ResolvedGlyph>>,
    /// Per-face variable font info: `(is_variable, has_opsz_axis)`.
    variable_fonts: HashMap<ID, (bool, bool)>,
}

impl GlyphCache {
    pub(crate) fn clear(&mut self) {
        self.glyphs.clear();
        self.variable_fonts.clear();
    }

    /// Returns `(is_variable, has_opsz_axis)` for a font face.
    fn variable_font_info(&mut self, fontdb: &Database, id: ID) -> (bool, bool) {
        *self
            .variable_fonts
            .entry(id)
            .or_insert_with(|| fontdb.variable_font_info(id))
    }

    fn glyph(&mut self, fontdb: &Database, id: ID, glyph_id: GlyphId) -> Option<ResolvedGlyph> {
        self.glyphs
            .entry((id, glyph_id))
            .or_insert_with(|| fontdb.glyph(id, glyph_id))
            .clone()
    }
}

impl std::fmt::Debug for GlyphCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlyphCache")
            .field("len", &self.glyphs.len())
            .finish()
    }
}

pub(crate) fn flatten(
    text: &mut Text,
    fontdb: &fontdb::Database,
    glyph_cache: &mut GlyphCache,
) -> Option<(Group, NonZeroRect)> {
    let mut new_children = vec![];

    let abs_transform = text.abs_transform;
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

        // For variable fonts, we need to extract the outline with variations applied.
        // We can't use the glyph cache here since the outline depends on variation values.
        let has_explicit_variations = !span.variations.is_empty();

        for glyph in &span.positioned_glyphs {
            // The font face is parsed only once per glyph and the glyph is resolved
            // in the same order as upstream: COLR, SVG, bitmap and finally the outline.
            match glyph_cache.glyph(fontdb, glyph.font, glyph.id) {
                // A (best-effort conversion of a) COLR glyph.
                Some(ResolvedGlyph::Colr(tree)) => {
                    push_outline_paths(
                        span,
                        &mut span_builder,
                        &mut new_children,
                        rendering_mode,
                        abs_transform,
                    );

                    let mut group = Group {
                        transform: glyph.colr_transform(),
                        ..Group::empty()
                    };
                    // TODO: Probably need to update abs_transform of children? Same
                    // for SVG and bitmap glyphs.
                    group.children.push(Node::Group(Box::new(tree.root)));
                    group.calculate_bounding_boxes();

                    new_children.push(Node::Group(Box::new(group)));
                }
                // An SVG glyph. Will return the usvgr node containing the glyph descriptions.
                Some(ResolvedGlyph::Svg(node)) => {
                    push_outline_paths(
                        span,
                        &mut span_builder,
                        &mut new_children,
                        rendering_mode,
                        abs_transform,
                    );

                    let mut group = Group {
                        transform: glyph.svg_transform(),
                        ..Group::empty()
                    };
                    group.children.push(node);
                    group.calculate_bounding_boxes();

                    new_children.push(Node::Group(Box::new(group)));
                }
                // A bitmap glyph.
                Some(ResolvedGlyph::Raster(img)) => {
                    push_outline_paths(
                        span,
                        &mut span_builder,
                        &mut new_children,
                        rendering_mode,
                        abs_transform,
                    );

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
                    // Only bypass the cache for variable fonts with either explicit
                    // variations or auto optical sizing on a font with an `opsz` axis.
                    // Non-variable fonts ignore variations, so the cached outline is exact.
                    let (is_variable, has_opsz) =
                        glyph_cache.variable_font_info(fontdb, glyph.font);
                    let needs_variations = is_variable
                        && (has_explicit_variations
                            || (span.font_optical_sizing == crate::FontOpticalSizing::Auto
                                && has_opsz));

                    let outline = if needs_variations {
                        fontdb.outline_with_variations(
                            glyph.font,
                            glyph.id,
                            &span.variations,
                            glyph.font_size(),
                            span.font_optical_sizing,
                        )
                    } else {
                        Some(outline)
                    };

                    if let Some(outline) =
                        outline.and_then(|p| p.transform(glyph.outline_transform()))
                    {
                        span_builder.push_path(&outline);
                    }
                }
                None => {}
            }
        }

        push_outline_paths(
            span,
            &mut span_builder,
            &mut new_children,
            rendering_mode,
            abs_transform,
        );

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
    fn outline_with_variations(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        font_size: f32,
        font_optical_sizing: crate::FontOpticalSizing,
    ) -> Option<tiny_skia_path::Path>;
    /// Returns `(is_variable, has_opsz_axis)`.
    fn variable_font_info(&self, id: ID) -> (bool, bool);
}

/// A glyph resolved from a font face.
#[derive(Clone)]
pub(crate) enum ResolvedGlyph {
    /// A (best-effort conversion of a) COLR glyph.
    Colr(Tree),
    /// An SVG glyph.
    Svg(Node),
    /// A bitmap (`sbix`/`CBDT`) glyph.
    Raster(BitmapImage),
    /// A plain outline glyph (`glyf`, `CFF`, `CFF2`).
    Outline(tiny_skia_path::Path),
}

#[derive(Clone)]
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
            let mut font = ttf_parser::Face::parse(data, face_index).ok()?;
            let tables = font.tables();

            if tables.colr.is_some() {
                if let Some(tree) = colr(&font, glyph_id) {
                    return Some(ResolvedGlyph::Colr(tree));
                }
            }

            if tables.svg.is_some() {
                if let Some(node) = svg(&font, glyph_id) {
                    return Some(ResolvedGlyph::Svg(node));
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

            // For variable fonts, we need to set default variation values to get proper outlines
            if font.is_variable() {
                for axis in font.variation_axes() {
                    font.set_variation(axis.tag, axis.def_value);
                }
            }

            outline(&font, glyph_id).map(ResolvedGlyph::Outline)
        })?
    }

    #[inline(never)]
    fn outline_with_variations(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        font_size: f32,
        font_optical_sizing: crate::FontOpticalSizing,
    ) -> Option<tiny_skia_path::Path> {
        self.with_face_data(id, |data, face_index| -> Option<tiny_skia_path::Path> {
            let mut font = ttf_parser::Face::parse(data, face_index).ok()?;

            for v in variations {
                font.set_variation(ttf_parser::Tag::from_bytes(&v.tag), v.value);
            }

            // Auto-set opsz if font-optical-sizing is auto and not explicitly set
            if font_optical_sizing == crate::FontOpticalSizing::Auto {
                let has_explicit_opsz = variations.iter().any(|v| v.tag == *b"opsz");
                if !has_explicit_opsz && face_has_opsz_axis(&font) {
                    font.set_variation(ttf_parser::Tag::from_bytes(b"opsz"), font_size);
                }
            }

            outline(&font, glyph_id)
        })?
    }

    fn variable_font_info(&self, id: ID) -> (bool, bool) {
        self.with_face_data(id, |data, face_index| -> Option<(bool, bool)> {
            let font = ttf_parser::Face::parse(data, face_index).ok()?;
            Some((font.is_variable(), face_has_opsz_axis(&font)))
        })
        .flatten()
        .unwrap_or((false, false))
    }
}

fn face_has_opsz_axis(font: &ttf_parser::Face) -> bool {
    font.tables().fvar.map_or(false, |axes| {
        axes.axes
            .into_iter()
            .any(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"opsz"))
    })
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

fn svg(font: &ttf_parser::Face, glyph_id: GlyphId) -> Option<Node> {
    // TODO: Technically not 100% accurate because the SVG format in a OTF font
    // is actually a subset/superset of a normal SVG, but it seems to work fine
    // for Twitter Color Emoji, so might as well use what we already have.

    // TODO: Glyph records can contain the data for multiple glyphs. We should
    // add a cache so we don't need to reparse the data every time.
    let image = font.glyph_svg_image(glyph_id)?;
    let tree = Tree::from_data(image.data, &Options::default(), &fontdb::Database::new()).ok()?;

    // Twitter Color Emoji seems to always have one SVG record per glyph,
    // while Noto Color Emoji sometimes contains multiple ones. It's kind of hacky,
    // but the best we have for now.
    let node = if image.start_glyph_id == image.end_glyph_id {
        Node::Group(Box::new(tree.root))
    } else {
        tree.node_by_id(&format!("glyph{}", glyph_id.0))
            .log_none(|| log::warn!("Failed to find SVG glyph node for glyph {}", glyph_id.0))
            .cloned()?
    };

    Some(node)
}

fn colr(face: &ttf_parser::Face, glyph_id: GlyphId) -> Option<Tree> {
    let mut svg = XmlWriter::new(xmlwriter::Options::default());

    svg.start_element("svg");
    svg.write_attribute("xmlns", "http://www.w3.org/2000/svg");
    svg.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");

    let mut path_buf = String::with_capacity(256);
    let gradient_index = 1;
    let clip_path_index = 1;

    svg.start_element("g");

    let mut glyph_painter = GlyphPainter {
        face,
        svg: &mut svg,
        path_buf: &mut path_buf,
        gradient_index,
        clip_path_index,
        palette_index: 0,
        transform: ttf_parser::Transform::default(),
        outline_transform: ttf_parser::Transform::default(),
        transforms_stack: vec![ttf_parser::Transform::default()],
    };

    face.paint_color_glyph(
        glyph_id,
        0,
        RgbaColor::new(0, 0, 0, 255),
        &mut glyph_painter,
    )?;
    svg.end_element();

    Tree::from_data(
        svg.end_document().as_bytes(),
        &Options::default(),
        &fontdb::Database::new(),
    )
    .ok()
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
