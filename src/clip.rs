// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use usvgr::NodeExt;

use crate::{cache, render::Canvas, ConvTransform, OptionLog};

pub fn clip(
    tree: &usvgr::Tree,
    cp: &usvgr::ClipPath,
    bbox: usvgr::PathBbox,
    canvas: &mut Canvas,
    cache: &mut cache::SvgrCache,
) -> Option<()> {
    let mut clip_pixmap = tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height())?;
    clip_pixmap.fill(tiny_skia::Color::BLACK);

    let mut clip_canvas = Canvas::from(clip_pixmap.as_mut());
    clip_canvas.skip_caching = true;
    clip_canvas.transform = canvas.transform;
    clip_canvas.apply_transform(cp.transform.to_native());

    if cp.units == usvgr::Units::ObjectBoundingBox {
        let bbox = bbox
            .to_rect()
            .log_none(|| log::warn!("Clipping of zero-sized shapes is not allowed."))?;

        clip_canvas.apply_transform(usvgr::Transform::from_bbox(bbox).to_native());
    }

    let ts = clip_canvas.transform;
    for node in cp.root.children() {
        clip_canvas.apply_transform(node.transform().to_native());

        match *node.borrow() {
            usvgr::NodeKind::Path(ref path_node) => {
                crate::path::draw(
                    tree,
                    path_node,
                    &node,
                    tiny_skia::BlendMode::Clear,
                    &mut clip_canvas,
                    cache,
                );
            }
            usvgr::NodeKind::Group(ref g) => {
                clip_group(tree, &node, g, bbox, &mut clip_canvas, cache);
            }
            _ => {}
        }

        clip_canvas.transform = ts;
    }

    if let Some(ref cp) = cp.clip_path {
        clip(tree, cp, bbox, canvas, cache);
    }

    let mut paint = tiny_skia::PixmapPaint::default();
    paint.blend_mode = tiny_skia::BlendMode::DestinationOut;
    canvas.pixmap.draw_pixmap(
        0,
        0,
        clip_pixmap.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );

    Some(())
}

fn clip_group(
    tree: &usvgr::Tree,
    node: &usvgr::Node,
    g: &usvgr::Group,
    bbox: usvgr::PathBbox,
    canvas: &mut Canvas,
    cache: &mut cache::SvgrCache,
) -> Option<()> {
    if let Some(ref cp) = g.clip_path {
        // If a `clipPath` child also has a `clip-path`
        // then we should render this child on a new canvas,
        // clip it, and only then draw it to the `clipPath`.

        let mut clip_pixmap =
            tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height())?;
        let mut clip_canvas = Canvas::from(clip_pixmap.as_mut());
        clip_canvas.transform = canvas.transform;

        draw_group_child(tree, node, &mut clip_canvas, cache);
        clip(tree, cp, bbox, &mut clip_canvas, cache);

        let mut paint = tiny_skia::PixmapPaint::default();
        paint.blend_mode = tiny_skia::BlendMode::Xor;
        canvas.pixmap.draw_pixmap(
            0,
            0,
            clip_pixmap.as_ref(),
            &paint,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    Some(())
}

fn draw_group_child(
    tree: &usvgr::Tree,
    node: &usvgr::Node,
    canvas: &mut Canvas,
    cache: &mut cache::SvgrCache,
) {
    if let Some(child) = node.first_child() {
        canvas.apply_transform(child.transform().to_native());

        if let usvgr::NodeKind::Path(ref path_node) = *child.borrow() {
            crate::path::draw(
                tree,
                path_node,
                node,
                tiny_skia::BlendMode::SourceOver,
                canvas,
                cache,
            );
        }
    }
}
