// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use svgrtypes::{Length, LengthUnit as Unit, PaintOrderKind, TransformOrigin};
use tiny_skia_path::Transform;

#[cfg(feature = "text")]
use self::text_to_paths::UsvgrTextOutlineCache;

use super::svgtree::{self, AId, EId, FromValue, SvgNode};
use super::units::{self, convert_length};
use super::{marker, Error, Options};
use crate::parser::paint_server::process_paint;
use crate::svgtree::macro_prelude::MaybeTransform;
use crate::svgtree::SvgAttributeValueRef;
use crate::*;

#[derive(Clone, Debug)]
pub struct State<'a> {
    pub(crate) parent_clip_path: Option<SvgNode<'a, 'a>>,
    pub(crate) parent_markers: Vec<SvgNode<'a, 'a>>,
    /// Stores the resolved fill and stroke of a use node
    /// or a path element (for markers)
    pub(crate) context_element: Option<(Option<Fill>, Option<Stroke>)>,
    pub(crate) fe_image_link: bool,
    /// A viewBox of the parent SVG element.
    pub(crate) view_box: NonZeroRect,
    /// A size of the parent `use` element.
    /// Used only during nested `svg` size resolving.
    /// Width and height can be set independently.
    pub(crate) use_size: (Option<f32>, Option<f32>),
    pub(crate) opt: &'a Options<'a>,
    #[cfg(feature = "text")]
    pub(crate) fontdb: &'a fontdb::Database,
}

#[derive(Debug, Default)]
#[allow(missing_docs)]
pub struct Cache {
    #[cfg(feature = "text")]
    pub usvgr_text_cache: Option<UsvgrTextOutlineCache>,
    pub clip_paths: HashMap<String, Arc<ClipPath>>,
    pub masks: HashMap<String, Arc<Mask>>,
    pub filters: HashMap<String, Arc<filter::Filter>>,
    pub paint: HashMap<String, Paint>,

    // used for ID generation
    all_ids: HashSet<u64>,
    linear_gradient_index: usize,
    radial_gradient_index: usize,
    pattern_index: usize,
    clip_path_index: usize,
    mask_index: usize,
    filter_index: usize,
}

impl Cache {
    /// Cleans elements ids for the next SVG within the FFrames run
    /// Surprisingly test suite fails if you are not actually cleaning the cache
    /// meaning that it is not possible to reuse the ids between different runs for FFrames
    /// But we do not clean our own caches which are designed to optimize cross SVG processing
    pub fn clear(&mut self) {
        self.clip_paths.clear();
        self.masks.clear();
        self.filters.clear();
        self.all_ids.clear();
        self.paint.clear();

        self.linear_gradient_index = 0;
        self.radial_gradient_index = 0;
        self.pattern_index = 0;
        self.clip_path_index = 0;
        self.mask_index = 0;
        self.filter_index = 0;
    }

    /// Creates a new cache with an external text outline cache
    #[cfg(feature = "text")]
    pub fn new_with_text_cache(text_cache_capacity: usize) -> Self {
        Self {
            usvgr_text_cache: UsvgrTextOutlineCache::new(text_cache_capacity),
            ..Default::default()
        }
    }

    // TODO: macros?
    pub(crate) fn gen_linear_gradient_id(&mut self) -> NonEmptyString {
        loop {
            self.linear_gradient_index += 1;
            let new_id = format!("linearGradient{}", self.linear_gradient_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }

    pub(crate) fn gen_radial_gradient_id(&mut self) -> NonEmptyString {
        loop {
            self.radial_gradient_index += 1;
            let new_id = format!("radialGradient{}", self.radial_gradient_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }

    pub(crate) fn gen_pattern_id(&mut self) -> NonEmptyString {
        loop {
            self.pattern_index += 1;
            let new_id = format!("pattern{}", self.pattern_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }

    pub(crate) fn gen_clip_path_id(&mut self) -> NonEmptyString {
        loop {
            self.clip_path_index += 1;
            let new_id = format!("clipPath{}", self.clip_path_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }

    pub(crate) fn gen_mask_id(&mut self) -> NonEmptyString {
        loop {
            self.mask_index += 1;
            let new_id = format!("mask{}", self.mask_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }

    pub(crate) fn gen_filter_id(&mut self) -> NonEmptyString {
        loop {
            self.filter_index += 1;
            let new_id = format!("filter{}", self.filter_index);
            let new_hash = string_hash(&new_id);
            if !self.all_ids.contains(&new_hash) {
                return NonEmptyString::new(new_id).unwrap();
            }
        }
    }
}

// TODO: is there a simpler way?
fn string_hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[allow(missing_docs)]
impl<'a, 'input: 'a> SvgNode<'a, 'input> {
    pub(crate) fn convert_length(
        &self,
        aid: AId,
        object_units: Units,
        state: &State,
        def: Length,
    ) -> f32 {
        units::convert_length(
            self.attribute(aid).unwrap_or(def),
            *self,
            aid,
            object_units,
            state,
        )
    }

    pub fn convert_user_length(&self, aid: AId, state: &State, def: Length) -> f32 {
        self.convert_length(aid, Units::UserSpaceOnUse, state, def)
    }

    pub fn parse_viewbox(&self) -> Option<NonZeroRect> {
        let vb: svgrtypes::ViewBox = self.attribute(AId::ViewBox)?;
        NonZeroRect::from_xywh(vb.x as f32, vb.y as f32, vb.w as f32, vb.h as f32)
    }

    pub fn resolve_length(&self, aid: AId, state: &State, def: f32) -> f32 {
        debug_assert!(
            !matches!(aid, AId::BaselineShift | AId::FontSize),
            "{} cannot be resolved via this function",
            aid
        );

        if let Some(n) = self.ancestors().find(|n| n.has_attribute(aid)) {
            if let Some(length) = n.attribute(aid) {
                return units::convert_user_length(length, n, aid, state);
            }
        }

        def
    }

    pub fn resolve_valid_length(
        &self,
        aid: AId,
        state: &State,
        def: f32,
    ) -> Option<NonZeroPositiveF32> {
        let n = self.resolve_length(aid, state, def);
        NonZeroPositiveF32::new(n)
    }

    pub(crate) fn try_convert_length(
        &self,
        aid: AId,
        object_units: Units,
        state: &State,
    ) -> Option<f32> {
        Some(units::convert_length(
            self.attribute(aid)?,
            *self,
            aid,
            object_units,
            state,
        ))
    }

    pub fn has_valid_transform(&self, aid: AId) -> bool {
        // Do not use Node::attribute::<Transform>, because it will always
        // return a valid transform.
        match self.attribute::<MaybeTransform>(aid) {
            Some(MaybeTransform::Valid(_)) => true,
            // no transform is a valid transform
            None => true,
            _ => false,
        }
    }

    pub fn is_visible_element(&self, opt: &crate::Options) -> bool {
        self.attribute(AId::Display) != Some("none")
            && self.has_valid_transform(AId::Transform)
            && super::switch::is_condition_passed(*self, opt)
    }
}

pub trait SvgColorExt {
    fn split_alpha(self) -> (Color, Opacity);
}

impl SvgColorExt for svgrtypes::Color {
    fn split_alpha(self) -> (Color, Opacity) {
        (
            Color::new_rgb(self.red, self.green, self.blue),
            Opacity::new_u8(self.alpha),
        )
    }
}

/// Converts an input `Document` into a `Tree`.
///
/// # Errors
///
/// - If `Document` doesn't have an SVG node - returns an empty tree.
/// - If `Document` doesn't have a valid size - returns `Error::InvalidSize`.
pub(crate) fn convert_doc(
    svg_doc: &svgtree::Document,
    opt: &Options,
    cache: &mut Cache,
    #[cfg(feature = "text")] fontdb: &fontdb::Database,
) -> Result<Tree, Error> {
    let svg = svg_doc.root_element();
    let (size, restore_viewbox) = resolve_svg_size(
        &svg,
        opt,
        #[cfg(feature = "text")]
        fontdb,
    );
    let size = size?;
    let view_box = ViewBox {
        rect: svg
            .parse_viewbox()
            .unwrap_or_else(|| size.to_non_zero_rect(0.0, 0.0)),
        aspect: svg.attribute(AId::PreserveAspectRatio).unwrap_or_default(),
    };

    let mut tree = Tree {
        size,
        view_box,
        root: Group::empty(),
        linear_gradients: Vec::new(),
        radial_gradients: Vec::new(),
        patterns: Vec::new(),
        clip_paths: Vec::new(),
        masks: Vec::new(),
        filters: Vec::new(),
    };

    if !svg.is_visible_element(opt) {
        return Ok(tree);
    }

    let state = State {
        parent_clip_path: None,
        context_element: None,
        parent_markers: Vec::new(),
        fe_image_link: false,
        view_box: view_box.rect,
        use_size: (None, None),
        opt,
        #[cfg(feature = "text")]
        fontdb,
    };

    for node in svg_doc.descendants() {
        if let Some(tag) = node.tag_name() {
            if matches!(
                tag,
                EId::ClipPath
                    | EId::Filter
                    | EId::LinearGradient
                    | EId::Mask
                    | EId::Pattern
                    | EId::RadialGradient
            ) && !node.element_id().is_empty()
            {
                cache.all_ids.insert(string_hash(node.element_id()));
            }
        }
    }

    convert_children(svg_doc.root(), &state, cache, &mut tree.root);
    cache.clear();

    super::paint_server::update_paint_servers(
        &mut tree.root,
        Transform::default(),
        None,
        None,
        cache,
    );
    tree.collect_paint_servers();
    tree.root.collect_clip_paths(&mut tree.clip_paths);
    tree.root.collect_masks(&mut tree.masks);
    tree.root.collect_filters(&mut tree.filters);
    tree.root.calculate_bounding_boxes();

    if restore_viewbox {
        calculate_svg_bbox(&mut tree);
    }

    Ok(tree)
}

fn resolve_svg_size(
    svg: &SvgNode,
    opt: &Options,
    #[cfg(feature = "text")] fontdb: &fontdb::Database,
) -> (Result<Size, Error>, bool) {
    let mut state = State {
        parent_clip_path: None,
        context_element: None,
        parent_markers: Vec::new(),
        fe_image_link: false,
        view_box: NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
        use_size: (None, None),
        opt,
        #[cfg(feature = "text")]
        fontdb,
    };

    let def = Length::new(100.0, Unit::Percent);
    let mut width: Length = svg.attribute(AId::Width).unwrap_or(def);
    let mut height: Length = svg.attribute(AId::Height).unwrap_or(def);

    let view_box = svg.parse_viewbox();

    let restore_viewbox =
        if (width.unit == Unit::Percent || height.unit == Unit::Percent) && view_box.is_none() {
            // Apply the percentages to the fallback size.
            if width.unit == Unit::Percent {
                width = Length::new(
                    (width.number / 100.0) * state.opt.default_size.width() as f64,
                    Unit::None,
                );
            }

            if height.unit == Unit::Percent {
                height = Length::new(
                    (height.number / 100.0) * state.opt.default_size.height() as f64,
                    Unit::None,
                );
            }

            true
        } else {
            false
        };

    let size = if let Some(vbox) = view_box {
        state.view_box = vbox;

        let w = if width.unit == Unit::Percent {
            vbox.width() * (width.number as f32 / 100.0)
        } else {
            svg.convert_user_length(AId::Width, &state, def)
        };

        let h = if height.unit == Unit::Percent {
            vbox.height() * (height.number as f32 / 100.0)
        } else {
            svg.convert_user_length(AId::Height, &state, def)
        };

        Size::from_wh(w, h)
    } else {
        Size::from_wh(
            svg.convert_user_length(AId::Width, &state, def),
            svg.convert_user_length(AId::Height, &state, def),
        )
    };

    (size.ok_or(Error::InvalidSize), restore_viewbox)
}

/// Calculates SVG's size and viewBox in case there were not set.
///
/// Simply iterates over all nodes and calculates a bounding box.
fn calculate_svg_bbox(tree: &mut Tree) {
    let bbox = tree.root.abs_bounding_box();

    if let Some(rect) = NonZeroRect::from_xywh(0.0, 0.0, bbox.right(), bbox.bottom()) {
        tree.view_box.rect = rect;
    }

    if let Some(size) = Size::from_wh(bbox.right(), bbox.bottom()) {
        tree.size = size;
    }
}

#[inline(never)]
pub(crate) fn convert_children(
    parent_node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    for node in parent_node.children() {
        convert_element(node, state, cache, parent);
    }
}

#[inline(never)]
pub(crate) fn convert_element(node: SvgNode, state: &State, cache: &mut Cache, parent: &mut Group) {
    let tag_name = match node.tag_name() {
        Some(v) => v,
        None => return,
    };

    if !tag_name.is_graphic() && !matches!(tag_name, EId::G | EId::Switch | EId::Svg) {
        return;
    }

    if !node.is_visible_element(state.opt) {
        return;
    }

    if tag_name == EId::Use {
        super::use_node::convert(node, state, cache, parent);
        return;
    }

    if tag_name == EId::Switch {
        super::switch::convert(node, state, cache, parent);
        return;
    }

    if let Some(g) = convert_group(node, state, false, cache, parent, &|cache, g| {
        convert_element_impl(tag_name, node, state, cache, g);
    }) {
        parent.children.push(Node::Group(Box::new(g)));
    }
}

#[inline(never)]
fn convert_element_impl(
    tag_name: EId,
    node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    match tag_name {
        EId::Rect
        | EId::Circle
        | EId::Ellipse
        | EId::Line
        | EId::Polyline
        | EId::Polygon
        | EId::Path => {
            if let Some(path) = super::shapes::convert(node, state) {
                convert_path(node, path, state, cache, parent);
            }
        }
        EId::Image => {
            super::image::convert(node, state, parent);
        }
        EId::Text => {
            #[cfg(feature = "text")]
            {
                super::text::convert(node, state, cache, parent);
            }
        }
        EId::Svg => {
            if node.parent_element().is_some() {
                super::use_node::convert_svg(node, state, cache, parent);
            } else {
                // Skip root `svg`.
                convert_children(node, state, cache, parent);
            }
        }
        EId::G => {
            convert_children(node, state, cache, parent);
        }
        _ => {}
    }
}

// `clipPath` can have only shape and `text` children.
//
// `line` doesn't impact rendering because stroke is always disabled
// for `clipPath` children.
#[inline(never)]
pub(crate) fn convert_clip_path_elements(
    clip_node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    for node in clip_node.children() {
        let tag_name = match node.tag_name() {
            Some(v) => v,
            None => continue,
        };

        if !tag_name.is_graphic() {
            continue;
        }

        if !node.is_visible_element(state.opt) {
            continue;
        }

        if tag_name == EId::Use {
            super::use_node::convert(node, state, cache, parent);
            continue;
        }

        if let Some(g) = convert_group(node, state, false, cache, parent, &|cache, g| {
            convert_clip_path_elements_impl(tag_name, node, state, cache, g);
        }) {
            parent.children.push(Node::Group(Box::new(g)));
        }
    }
}

#[inline(never)]
fn convert_clip_path_elements_impl(
    tag_name: EId,
    node: SvgNode,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    match tag_name {
        EId::Rect | EId::Circle | EId::Ellipse | EId::Polyline | EId::Polygon | EId::Path => {
            if let Some(path) = super::shapes::convert(node, state) {
                convert_path(node, path, state, cache, parent);
            }
        }
        EId::Text => {
            #[cfg(feature = "text")]
            {
                super::text::convert(node, state, cache, parent);
            }
        }
        _ => {
            log::warn!("'{}' is no a valid 'clip-path' child.", tag_name);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Isolation {
    Auto,
    Isolate,
}

impl Default for Isolation {
    fn default() -> Self {
        Self::Auto
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Isolation {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        match value.as_str()? {
            "auto" => Some(Isolation::Auto),
            "isolate" => Some(Isolation::Isolate),
            _ => None,
        }
    }
}

// TODO: explain
pub(crate) fn convert_group(
    node: SvgNode,
    state: &State,
    force: bool,
    cache: &mut Cache,
    parent: &mut Group,
    collect_children: &dyn Fn(&mut Cache, &mut Group),
) -> Option<Group> {
    // A `clipPath` child cannot have an opacity.
    let opacity = if state.parent_clip_path.is_none() {
        node.attribute::<Opacity>(AId::Opacity)
            .unwrap_or(Opacity::ONE)
    } else {
        Opacity::ONE
    };

    let transform = node.resolve_transform(AId::Transform, state);
    let blend_mode: BlendMode = node.attribute(AId::MixBlendMode).unwrap_or_default();
    let isolation: Isolation = node.attribute(AId::Isolation).unwrap_or_default();
    let isolate = isolation == Isolation::Isolate;

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let is_g_or_use = matches!(node.tag_name(), Some(EId::G) | Some(EId::Use));
    let id = if is_g_or_use && state.parent_markers.is_empty() {
        node.element_id().to_string()
    } else {
        String::new()
    };

    let abs_transform = parent.abs_transform.pre_concat(transform);
    let dummy = Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap();
    let mut g = Group {
        id,
        transform,
        abs_transform,
        opacity,
        blend_mode,
        isolate,
        clip_path: None,
        mask: None,
        filters: Vec::new(),
        is_context_element: false,
        bounding_box: dummy,
        abs_bounding_box: dummy,
        stroke_bounding_box: dummy,
        abs_stroke_bounding_box: dummy,
        layer_bounding_box: NonZeroRect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap(),
        abs_layer_bounding_box: NonZeroRect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap(),
        children: Vec::new(),
    };
    collect_children(cache, &mut g);

    // We need to know group's bounding box before converting
    // clipPaths, masks and filters.
    let object_bbox = g.calculate_object_bbox();

    // `mask` and `filter` cannot be set on `clipPath` children.
    // But `clip-path` can.

    let mut clip_path = None;
    if let Some(link) = node.attribute::<SvgNode>(AId::ClipPath) {
        clip_path = super::clippath::convert(link, state, object_bbox, cache);
        if clip_path.is_none() {
            return None;
        }
    }

    let mut mask = None;
    if state.parent_clip_path.is_none() {
        if let Some(link) = node.attribute::<SvgNode>(AId::Mask) {
            mask = super::mask::convert(link, state, object_bbox, cache);
            if mask.is_none() {
                return None;
            }
        }
    }

    let filters = {
        let mut filters = Vec::new();
        if state.parent_clip_path.is_none() {
            if node.attribute(AId::Filter) == Some("none") {
                // Do nothing.
            } else if node.has_attribute(AId::Filter) {
                if let Ok(f) = super::filter::convert(node, state, object_bbox, cache) {
                    filters = f;
                } else {
                    // A filter that not a link or a filter with a link to a non existing element.
                    //
                    // Unlike `clip-path` and `mask`, when a `filter` link is invalid
                    // then the whole element should be ignored.
                    //
                    // This is kinda an undefined behaviour.
                    // In most cases, Chrome, Firefox and rsvg will ignore such elements,
                    // but in some cases Chrome allows it. Not sure why.
                    // Inkscape (0.92) simply ignores such attributes, rendering element as is.
                    // Batik (1.12) crashes.
                    //
                    // Test file: e-filter-051.svg
                    return None;
                }
            }
        }

        filters
    };

    let required = opacity.get().approx_ne_ulps(&1.0, 4)
        || clip_path.is_some()
        || mask.is_some()
        || !filters.is_empty()
        || !transform.is_identity()
        || blend_mode != BlendMode::Normal
        || isolate
        || is_g_or_use
        || force;

    if !required {
        parent.children.append(&mut g.children);
        return None;
    }

    g.clip_path = clip_path;
    g.mask = mask;
    g.filters = filters;

    // Must be called after we set Group::filters
    g.calculate_bounding_boxes();

    Some(g)
}

fn convert_path(
    node: SvgNode,
    tiny_skia_path: Arc<tiny_skia_path::Path>,
    state: &State,
    cache: &mut Cache,
    parent: &mut Group,
) {
    debug_assert!(tiny_skia_path.len() >= 2);
    if tiny_skia_path.len() < 2 {
        return;
    }

    let has_bbox = tiny_skia_path.bounds().width() > 0.0 && tiny_skia_path.bounds().height() > 0.0;
    let mut fill = super::style::resolve_fill(node, has_bbox, state, cache);
    let mut stroke = super::style::resolve_stroke(node, has_bbox, state, cache);
    let mut visibility: Visibility = node.find_attribute(AId::Visibility).unwrap_or_default();
    let rendering_mode: ShapeRendering = node
        .find_attribute(AId::ShapeRendering)
        .unwrap_or(state.opt.shape_rendering);

    // TODO: handle `markers` before `stroke`
    let raw_paint_order: svgrtypes::PaintOrder =
        node.find_attribute(AId::PaintOrder).unwrap_or_default();
    let paint_order = svg_paint_order_to_usvgr(raw_paint_order);
    let path_transform = parent.abs_transform;

    // If a path doesn't have a fill or a stroke then it's invisible.
    // By setting `visibility` to `hidden` we are disabling rendering of this path.
    if fill.is_none() && stroke.is_none() {
        visibility = Visibility::Hidden;
    }

    if let Some(fill) = fill.as_mut() {
        if let Some(ContextElement::PathNode(context_transform, context_bbox)) =
            fill.context_element
        {
            process_paint(
                &mut fill.paint,
                true,
                context_transform,
                context_bbox.map(|r| r.to_rect()),
                path_transform,
                tiny_skia_path.bounds(),
                cache,
            );
            fill.context_element = None;
        }
    }

    if let Some(stroke) = stroke.as_mut() {
        if let Some(ContextElement::PathNode(context_transform, context_bbox)) =
            stroke.context_element
        {
            process_paint(
                &mut stroke.paint,
                true,
                context_transform,
                context_bbox.map(|r| r.to_rect()),
                path_transform,
                tiny_skia_path.bounds(),
                cache,
            );
            stroke.context_element = None;
        }
    }

    let mut marker = None;
    if marker::is_valid(node) && visibility == Visibility::Visible {
        let mut marker_group = Group {
            abs_transform: parent.abs_transform,
            ..Group::empty()
        };
        let mut marker_state = state.clone();

        let bbox = tiny_skia_path
            .compute_tight_bounds()
            .and_then(|r| r.to_non_zero_rect());

        let fill = fill.clone().map(|mut f| {
            f.context_element = Some(ContextElement::PathNode(path_transform, bbox));
            f
        });

        let stroke = stroke.clone().map(|mut s| {
            s.context_element = Some(ContextElement::PathNode(path_transform, bbox));
            s
        });

        marker_state.context_element = Some((fill, stroke));

        marker::convert(
            node,
            &tiny_skia_path,
            &marker_state,
            cache,
            &mut marker_group,
        );
        marker_group.calculate_bounding_boxes();
        marker = Some(marker_group);
    }

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let id = if state.parent_markers.is_empty() {
        node.element_id().to_string()
    } else {
        String::new()
    };

    let path = Path::new(
        id,
        visibility,
        fill,
        stroke,
        paint_order,
        rendering_mode,
        tiny_skia_path,
        path_transform,
    );

    let path = match path {
        Some(v) => v,
        None => return,
    };

    match raw_paint_order.order {
        [PaintOrderKind::Markers, _, _] => {
            if let Some(markers_node) = marker {
                parent.children.push(Node::Group(Box::new(markers_node)));
            }

            parent.children.push(Node::Path(Box::new(path.clone())));
        }
        [first, PaintOrderKind::Markers, last] => {
            append_single_paint_path(first, &path, parent);

            if let Some(markers_node) = marker {
                parent.children.push(Node::Group(Box::new(markers_node)));
            }

            append_single_paint_path(last, &path, parent);
        }
        [_, _, PaintOrderKind::Markers] => {
            parent.children.push(Node::Path(Box::new(path.clone())));

            if let Some(markers_node) = marker {
                parent.children.push(Node::Group(Box::new(markers_node)));
            }
        }
        _ => parent.children.push(Node::Path(Box::new(path.clone()))),
    }
}

fn append_single_paint_path(paint_order_kind: PaintOrderKind, path: &Path, parent: &mut Group) {
    match paint_order_kind {
        PaintOrderKind::Fill => {
            if path.fill.is_some() {
                let mut fill_path = path.clone();
                fill_path.stroke = None;
                fill_path.id = String::new();
                parent.children.push(Node::Path(Box::new(fill_path)));
            }
        }
        PaintOrderKind::Stroke => {
            if path.stroke.is_some() {
                let mut stroke_path = path.clone();
                stroke_path.fill = None;
                stroke_path.id = String::new();
                parent.children.push(Node::Path(Box::new(stroke_path)));
            }
        }
        _ => {}
    }
}

pub fn svg_paint_order_to_usvgr(order: svgrtypes::PaintOrder) -> PaintOrder {
    match (order.order[0], order.order[1]) {
        (svgrtypes::PaintOrderKind::Stroke, _) => PaintOrder::StrokeAndFill,
        (svgrtypes::PaintOrderKind::Markers, svgrtypes::PaintOrderKind::Stroke) => {
            PaintOrder::StrokeAndFill
        }
        _ => PaintOrder::FillAndStroke,
    }
}

impl SvgNode<'_, '_> {
    pub(crate) fn resolve_transform(&self, transform_aid: AId, state: &State) -> Transform {
        let mut transform: Transform = self.attribute(transform_aid).unwrap_or_default();
        let transform_origin: Option<TransformOrigin> = self.attribute(AId::TransformOrigin);

        if let Some(transform_origin) = transform_origin {
            let dx = convert_length(
                transform_origin.x_offset,
                *self,
                AId::Width,
                Units::UserSpaceOnUse,
                state,
            );
            let dy = convert_length(
                transform_origin.y_offset,
                *self,
                AId::Height,
                Units::UserSpaceOnUse,
                state,
            );
            transform = Transform::default()
                .pre_translate(dx, dy)
                .pre_concat(transform)
                .pre_translate(-dx, -dy);
        }

        transform
    }
}
