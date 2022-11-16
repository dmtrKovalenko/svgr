// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::convert::TryInto;

use tiny_skia::{PixmapMut, PixmapPaint};
use usvgr::{filter::Blend, FuzzyEq, NodeExt, Transform};

use crate::ConvTransform;

pub struct Canvas<'a> {
    pub(crate) skip_caching: bool,
    pub pixmap: tiny_skia::PixmapMut<'a>,
    pub transform: tiny_skia::Transform,
    pub clip: Option<tiny_skia::ClipMask>,
}

pub struct FromPixmap {
    pub pixmap: tiny_skia::Pixmap,
    pub tx: i32,
    pub ty: i32,
    pub opacity: f32,
}

impl<'a> From<tiny_skia::PixmapMut<'a>> for Canvas<'a> {
    fn from(pixmap: tiny_skia::PixmapMut<'a>) -> Self {
        Canvas {
            pixmap,
            skip_caching: false,
            transform: tiny_skia::Transform::identity(),
            clip: None,
        }
    }
}

impl Canvas<'_> {
    pub fn translate(&mut self, tx: f32, ty: f32) {
        self.transform = self.transform.pre_translate(tx, ty);
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.transform = self.transform.pre_scale(sx, sy);
    }

    pub fn apply_transform(&mut self, ts: tiny_skia::Transform) {
        self.transform = self.transform.pre_concat(ts);
    }

    pub fn set_clip_rect(&mut self, rect: tiny_skia::Rect) {
        let path = tiny_skia::PathBuilder::from_rect(rect);
        if let Some(path) = path.transform(self.transform) {
            let mut clip = tiny_skia::ClipMask::new();
            clip.set_path(
                self.pixmap.width(),
                self.pixmap.height(),
                &path,
                tiny_skia::FillRule::Winding,
                true,
            );
            self.clip = Some(clip);
        }
    }

    pub fn merge_canvas(&mut self, other: &Canvas) {
        let self_data = self.pixmap.pixels_mut();
        let other_data = other.pixmap.as_ref().pixels();

        for (i, pixel) in other_data.iter().enumerate() {
            if pixel.get() > 0 {
                self_data[i] = *pixel
            }
        }
    }

    /// Creates sub pixmap that will be cached itself withing a canvas cache. Guarantees empty canvas within closure.  
    pub fn with_subpixmap_cache(canvas: &mut Canvas, mut f: impl FnMut(&mut Canvas) -> FromPixmap) {
        let mut pixmap =
            tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height()).unwrap();
        let pixmap_mut = pixmap.as_mut();

        let mut temp_canvas = Canvas {
            pixmap: pixmap_mut,
            transform: canvas.transform,
            skip_caching: true,
            clip: canvas.clip.clone(),
        };

        let FromPixmap {
            pixmap,
            tx,
            ty,
            opacity,
        } = f(&mut temp_canvas);

        canvas.pixmap.draw_pixmap(
            tx,
            ty,
            pixmap.as_ref(),
            &PixmapPaint {
                opacity,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            },
            tiny_skia::Transform::default(),
            None,
        );
    }

    pub fn with_cache(canvas: &mut Canvas, mut f: impl FnMut(&mut Canvas)) {
        if canvas.skip_caching {
            return f(canvas);
        }

        // safe to unwrap – cloning a pixmap can't fail for dimensions validation.
        let mut pixmap =
            tiny_skia::Pixmap::new(canvas.pixmap.width(), canvas.pixmap.height()).unwrap();
        let pixmap_mut = pixmap.as_mut();

        let mut temp_canvas = Canvas {
            pixmap: pixmap_mut,
            transform: canvas.transform,
            skip_caching: true,
            clip: canvas.clip.clone(),
        };

        f(&mut temp_canvas);
        canvas.pixmap.draw_pixmap(
            0,
            0,
            temp_canvas.pixmap.as_ref(),
            &PixmapPaint {
                opacity: 1.0,
                blend_mode: tiny_skia::BlendMode::SourceOver,
                quality: tiny_skia::FilterQuality::Nearest,
            },
            tiny_skia::Transform::default(),
            None,
        );
    }
}

/// Indicates the current rendering state.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum RenderState {
    /// A default value. Doesn't indicate anything.
    Ok,
    /// Indicates that the current rendering task should stop after reaching the specified node.
    #[allow(dead_code)]
    RenderUntil(usvgr::Node),
    /// Indicates that `usvgr::FilterInput::BackgroundImage` rendering task was finished.
    BackgroundFinished,
}

pub(crate) fn render_to_canvas(
    tree: &usvgr::Tree,
    img_size: usvgr::ScreenSize,
    canvas: &mut Canvas,
) {
    render_node_to_canvas(
        tree,
        &tree.root,
        tree.view_box,
        img_size,
        &mut RenderState::Ok,
        canvas,
    );
}

pub(crate) fn render_node_to_canvas(
    tree: &usvgr::Tree,
    node: &usvgr::Node,
    view_box: usvgr::ViewBox,
    img_size: usvgr::ScreenSize,
    state: &mut RenderState,
    canvas: &mut Canvas,
) {
    apply_viewbox_transform(view_box, img_size, canvas);

    let curr_ts = canvas.transform;

    let ts = node.abs_transform();

    canvas.apply_transform(ts.to_native());
    render_node(tree, node, state, canvas);
    canvas.transform = curr_ts;
}

/// Applies viewbox transformation to the painter.
fn apply_viewbox_transform(
    view_box: usvgr::ViewBox,
    img_size: usvgr::ScreenSize,
    canvas: &mut Canvas,
) {
    let ts =
        usvgr::utils::view_box_to_transform(view_box.rect, view_box.aspect, img_size.to_size());
    canvas.apply_transform(ts.to_native());
}

pub(crate) fn render_node(
    tree: &usvgr::Tree,
    node: &usvgr::Node,
    state: &mut RenderState,
    canvas: &mut Canvas,
) -> Option<usvgr::PathBbox> {
    match *node.borrow() {
        usvgr::NodeKind::Path(ref path) => {
            crate::path::draw(tree, path, tiny_skia::BlendMode::SourceOver, canvas)
        }
        usvgr::NodeKind::Image(ref img) => Some(crate::image::draw(img, canvas)),
        usvgr::NodeKind::Group(ref g) => render_group_impl(tree, node, g, state, canvas),
    }
}

pub(crate) fn render_group(
    tree: &usvgr::Tree,
    parent: &usvgr::Node,
    state: &mut RenderState,
    canvas: &mut Canvas,
) -> Option<usvgr::PathBbox> {
    let curr_ts = canvas.transform;
    let mut g_bbox = usvgr::PathBbox::new_bbox();

    for node in parent.children() {
        match state {
            RenderState::Ok => {}
            RenderState::RenderUntil(ref last) => {
                if node == *last {
                    // Stop rendering.
                    *state = RenderState::BackgroundFinished;
                    break;
                }
            }
            RenderState::BackgroundFinished => break,
        }

        canvas.apply_transform(node.transform().to_native());

        let bbox = render_node(tree, &node, state, canvas);
        if let Some(bbox) = bbox {
            if let Some(bbox) = bbox.transform(&node.transform()) {
                g_bbox = g_bbox.expand(bbox);
            }
        }

        // Revert transform.
        canvas.transform = curr_ts;
    }

    // Check that bbox was changed, otherwise we will have a rect with x/y set to f64::MAX.
    if g_bbox.fuzzy_ne(&usvgr::PathBbox::new_bbox()) {
        Some(g_bbox)
    } else {
        None
    }
}

fn render_group_impl(
    tree: &usvgr::Tree,
    node: &usvgr::Node,
    g: &usvgr::Group,
    state: &mut RenderState,
    canvas: &mut Canvas,
) -> Option<usvgr::PathBbox> {
    let mut bbox: Option<usvgr::PathBbox> = None;
    let curr_ts = canvas.transform;

    Canvas::with_subpixmap_cache(canvas, |sub_canvas| {
        bbox = render_group(tree, node, state, sub_canvas);

        // At this point, `sub_pixmap` has probably the same size as the viewbox.
        // So instead of clipping, masking and blending the whole viewbox, which can be very expensive,
        // we're trying to reduce `sub_pixmap` to it's actual content trimming
        // all transparent borders.
        //
        // Basically, if viewbox is 2000x2000 and the current group is 20x20, there is no point
        // in blending the whole viewbox, we can blend just the current group region.
        //
        // Transparency trimming is not yet allowed on groups with filter,
        // because filter expands the pixmap and it should be handled separately.
        #[cfg(feature = "filter")]
        let (tx, ty, mut sub_pixmap) = if g.filters.is_empty() {
            trim_transparency(&mut sub_canvas.pixmap).unwrap()
        } else {
            (0, 0, sub_canvas.pixmap.to_owned())
        };

        #[cfg(not(feature = "filter"))]
        let (tx, ty, mut sub_pixmap) = (0, 0, sub_pixmap);

        // During the background rendering for filters,
        // an opacity, a filter, a clip and a mask should be ignored for the inner group.
        // So we are simply rendering the `sub_img` without any postprocessing.
        //
        // SVG spec, 15.6 Accessing the background image
        // 'Any filter effects, masking and group opacity that might be set on A[i] do not apply
        // when rendering the children of A[i] into BUF[i].'
        if *state == RenderState::BackgroundFinished {
            return FromPixmap {
                opacity: 1.0,
                pixmap: sub_pixmap,
                tx,
                ty,
            };
        }

        // Filter can be rendered on an object without a bbox,
        // as long as filter uses `userSpaceOnUse`.
        #[cfg(feature = "filter")]
        for filter in &g.filters {
            let bbox = bbox.and_then(|r| r.to_rect());
            let ts = usvgr::Transform::from_native(curr_ts);
            let background = prepare_filter_background(tree, node, filter, &sub_pixmap);
            let fill_paint = prepare_filter_fill_paint(tree, node, filter, bbox, ts, &sub_pixmap);
            let stroke_paint =
                prepare_filter_stroke_paint(tree, node, filter, bbox, ts, &sub_pixmap);
            crate::filter::apply(
                filter,
                bbox,
                &ts,
                tree,
                background.as_ref(),
                fill_paint.as_ref(),
                stroke_paint.as_ref(),
                &mut sub_pixmap,
            )
        }

        // Clipping and masking can be done only for objects with a valid bbox.
        if let Some(bbox) = bbox {
            if let Some(ref clip_path) = g.clip_path {
                let mut sub_canvas = Canvas::from(sub_pixmap.as_mut());
                sub_canvas.skip_caching = true;
                sub_canvas.translate(-tx as f32, -ty as f32);
                sub_canvas.apply_transform(curr_ts);
                crate::clip::clip(tree, clip_path, bbox, &mut sub_canvas);
            }

            if let Some(ref mask) = g.mask {
                let mut sub_canvas = Canvas::from(sub_pixmap.as_mut());
                sub_canvas.skip_caching = true;
                sub_canvas.translate(-tx as f32, -ty as f32);
                sub_canvas.apply_transform(curr_ts);
                crate::mask::mask(tree, mask, bbox, &mut sub_canvas);
            }
        }

        let opacity = if g.opacity != usvgr::Opacity::ONE {
            g.opacity.get() as f32
        } else {
            1.0
        };

        FromPixmap {
            opacity,
            pixmap: sub_pixmap,
            tx,
            ty,
        }
    });

    bbox
}

/// Removes transparent borders from the image leaving only a tight bbox content.
///
/// Detects graphics element bbox on the raster images in absolute coordinates.
///
/// The current implementation is extremely simple and fairly slow.
/// Ideally, we should calculate the absolute bbox based on the current transform and bbox.
/// But because of anti-aliasing, float precision and especially stroking,
/// this can be fairly complicated and error-prone.
/// So for now we're using this method.
pub fn trim_transparency(
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<(i32, i32, tiny_skia::Pixmap)> {
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let mut min_x = pixmap.width() as i32;
    let mut min_y = pixmap.height() as i32;
    let pixels = pixmap.data_mut();
    let mut max_x = 0;
    let mut max_y = 0;

    let first_non_zero = {
        let max_safe_index = pixels.len() / 8;

        // Find first non-zero byte by looking at 8 bytes a time. If not found
        // checking the remaining bytes. This is a lot faster than checking one
        // byte a time.
        (0..max_safe_index)
            .position(|i| {
                let idx = i * 8;
                u64::from_ne_bytes((&pixels[idx..(idx + 8)]).try_into().unwrap()) != 0
            })
            .map_or_else(
                || ((max_safe_index * 8)..pixels.len()).position(|i| pixels[i] != 0),
                |i| Some(i * 8),
            )
    };

    // We skip all the transparent pixels at the beginning of the image. It's
    // very likely that transparent pixels all have rgba(0, 0, 0, 0) so skipping
    // zero bytes can be used as a quick optimization.
    // If the entire image is transparent, we don't need to continue.
    if first_non_zero != None {
        let get_alpha = |x, y| pixels[((width * y + x) * 4 + 3) as usize];

        // Find the top boundary.
        let start_y = first_non_zero.unwrap() as i32 / 4 / width;
        'top: for y in start_y..height {
            for x in 0..width {
                if get_alpha(x, y) != 0 {
                    min_x = x;
                    max_x = x;
                    min_y = y;
                    max_y = y;
                    break 'top;
                }
            }
        }

        // Find the bottom boundary.
        'bottom: for y in (max_y..height).rev() {
            for x in 0..width {
                if get_alpha(x, y) != 0 {
                    max_y = y;
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    break 'bottom;
                }
            }
        }

        // Find the left boundary.
        'left: for x in 0..min_x {
            for y in min_y..max_y {
                if get_alpha(x, y) != 0 {
                    min_x = x;
                    break 'left;
                }
            }
        }

        // Find the right boundary.
        'right: for x in (max_x..width).rev() {
            for y in min_y..max_y {
                if get_alpha(x, y) != 0 {
                    max_x = x;
                    break 'right;
                }
            }
        }
    }

    // Expand in all directions by 1px.
    min_x = (min_x - 1).max(0);
    min_y = (min_y - 1).max(0);
    max_x = (max_x + 2).min(pixmap.width() as i32);
    max_y = (max_y + 2).min(pixmap.height() as i32);

    if min_x < max_x && min_y < max_y {
        let rect = tiny_skia::IntRect::from_ltrb(min_x, min_y, max_x, max_y)?;
        let pixmap = pixmap.as_ref().clone_rect(rect)?;
        Some((min_x, min_y, pixmap))
    } else {
        Some((0, 0, pixmap.to_owned()))
    }
}

/// Renders an image used by `BackgroundImage` or `BackgroundAlpha` filter inputs.
#[cfg(feature = "filter")]
fn prepare_filter_background(
    tree: &usvgr::Tree,
    parent: &usvgr::Node,
    filter: &usvgr::filter::Filter,
    pixmap: &tiny_skia::Pixmap,
) -> Option<tiny_skia::Pixmap> {
    let start_node = parent.filter_background_start_node(filter)?;

    let img_size = usvgr::ScreenSize::new(pixmap.width(), pixmap.height()).unwrap();

    let mut pixmap = tiny_skia::Pixmap::new(pixmap.width(), pixmap.height()).unwrap();
    let mut canvas = Canvas::from(pixmap.as_mut());

    // Render from the `start_node` until the `parent`. The `parent` itself is excluded.
    let mut state = RenderState::RenderUntil(parent.clone());
    crate::render::render_node_to_canvas(
        tree,
        &start_node,
        tree.view_box,
        img_size,
        &mut state,
        &mut canvas,
    );

    Some(pixmap)
}

/// Renders an image used by `FillPaint`/`StrokePaint` filter input.
///
/// FillPaint/StrokePaint is mostly an undefined behavior and will produce different results
/// in every application.
/// And since there are no expected behaviour, we will simply fill the filter region.
///
/// https://github.com/w3c/fxtf-drafts/issues/323
#[cfg(feature = "filter")]
fn prepare_filter_fill_paint(
    tree: &usvgr::Tree,
    parent: &usvgr::Node,
    filter: &usvgr::filter::Filter,
    bbox: Option<usvgr::Rect>,
    ts: usvgr::Transform,
    pixmap: &tiny_skia::Pixmap,
) -> Option<tiny_skia::Pixmap> {
    let region = crate::filter::calc_region(filter, bbox, &ts, pixmap).ok()?;
    let mut sub_pixmap = tiny_skia::Pixmap::new(region.width(), region.height()).unwrap();
    let mut sub_canvas = Canvas::from(sub_pixmap.as_mut());
    if let usvgr::NodeKind::Group(ref g) = *parent.borrow() {
        if let Some(paint) = g.filter_fill.clone() {
            let style_bbox = bbox.unwrap_or_else(|| usvgr::Rect::new(0.0, 0.0, 1.0, 1.0).unwrap());

            let rect = tiny_skia::Rect::from_xywh(
                0.0,
                0.0,
                region.width() as f32,
                region.height() as f32,
            )?;
            let path = tiny_skia::PathBuilder::from_rect(rect);

            let fill = usvgr::Fill::from_paint(paint);
            crate::paint_server::fill(
                tree,
                &fill,
                style_bbox.to_path_bbox(),
                &path,
                true,
                tiny_skia::BlendMode::SourceOver,
                &mut sub_canvas,
            );
        }
    }

    Some(sub_pixmap)
}

/// The same as `prepare_filter_fill_paint`, but for `StrokePaint`.
#[cfg(feature = "filter")]
fn prepare_filter_stroke_paint(
    tree: &usvgr::Tree,
    parent: &usvgr::Node,
    filter: &usvgr::filter::Filter,
    bbox: Option<usvgr::Rect>,
    ts: usvgr::Transform,
    pixmap: &tiny_skia::Pixmap,
) -> Option<tiny_skia::Pixmap> {
    let region = crate::filter::calc_region(filter, bbox, &ts, pixmap).ok()?;
    let mut sub_pixmap = tiny_skia::Pixmap::new(region.width(), region.height()).unwrap();
    let mut sub_canvas = Canvas::from(sub_pixmap.as_mut());
    if let usvgr::NodeKind::Group(ref g) = *parent.borrow() {
        if let Some(paint) = g.filter_stroke.clone() {
            let style_bbox = bbox.unwrap_or_else(|| usvgr::Rect::new(0.0, 0.0, 1.0, 1.0).unwrap());

            let rect = tiny_skia::Rect::from_xywh(
                0.0,
                0.0,
                region.width() as f32,
                region.height() as f32,
            )?;
            let path = tiny_skia::PathBuilder::from_rect(rect);

            let fill = usvgr::Fill::from_paint(paint);
            crate::paint_server::fill(
                tree,
                &fill,
                style_bbox.to_path_bbox(),
                &path,
                true,
                tiny_skia::BlendMode::SourceOver,
                &mut sub_canvas,
            );
        }
    }

    Some(sub_pixmap)
}
