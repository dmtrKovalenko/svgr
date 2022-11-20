// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{cache::SvgrCache, render::Canvas};

pub fn draw(
    tree: &usvgr::Tree,
    path: &usvgr::Path,
    parent: &usvgr::Node,
    blend_mode: tiny_skia::BlendMode,
    canvas: &mut Canvas,
    cache: &mut SvgrCache,
) -> Option<usvgr::PathBbox> {
    let bbox = path.data.bbox();
    if path.visibility != usvgr::Visibility::Visible {
        return bbox;
    }

    let skia_path = convert_path(&path.data)?;

    cache.with_cache(canvas, parent, |canvas, cache| {
        // `usvgr` guaranties that path without a bbox will not use
        // a paint server with ObjectBoundingBox,
        // so we can pass whatever rect we want, because it will not be used anyway.
        let style_bbox = bbox.unwrap_or_else(|| usvgr::PathBbox::new(0.0, 0.0, 1.0, 1.0).unwrap());
        let antialias = path.rendering_mode.use_shape_antialiasing();

        let fill_path = |canvas, cache| {
            if let Some(ref fill) = path.fill {
                crate::paint_server::fill(
                    tree, fill, style_bbox, &skia_path, antialias, blend_mode, canvas, cache,
                );
            }
        };

        let stroke_path = |canvas, cache| {
            if path.stroke.is_some() {
                crate::paint_server::stroke(
                    tree,
                    &path.stroke,
                    style_bbox,
                    &skia_path,
                    antialias,
                    blend_mode,
                    canvas,
                    cache,
                );
            }
        };

        if path.paint_order == usvgr::PaintOrder::FillAndStroke {
            fill_path(canvas, cache);
            stroke_path(canvas, cache);
        } else {
            stroke_path(canvas, cache);
            fill_path(canvas, cache);
        }
    });

    bbox
}

fn convert_path(path: &usvgr::PathData) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for seg in path.segments() {
        match seg {
            usvgr::PathSegment::MoveTo { x, y } => {
                pb.move_to(x as f32, y as f32);
            }
            usvgr::PathSegment::LineTo { x, y } => {
                pb.line_to(x as f32, y as f32);
            }
            usvgr::PathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                pb.cubic_to(
                    x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
                );
            }
            usvgr::PathSegment::ClosePath => {
                pb.close();
            }
        }
    }

    pb.finish()
}
