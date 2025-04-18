// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::borrow::Cow;
use std::sync::Arc;
use svgrtypes::Length;

use super::svgtree::{AId, SvgNode};
use super::{converter, OptionLog};
use crate::svgtree::SvgAttributeValueRef;
use crate::{Group, Image, ImageKind, Node, NonZeroRect, Size, ViewBox};

#[derive(Debug, PartialEq)]
/// Preloaded decoded raster image data
pub struct PreloadedImageData {
    /// The decoded image data in RGBA format with blended semi trsparent color.
    /// Make sure that if you submit the data directly it must be blended for semi transparent colors.
    /// Either a static slice or an owned vector. Povide a static slice that is pre-blended in case
    /// of static resource linking for rendering.
    pub data: Cow<'static, [u8]>,
    /// The width of image in pixels
    pub width: u32,
    /// The height of image in pixels
    pub height: u32,
    /// Original id used to resolve the image
    pub id: String,
}

impl std::fmt::Display for PreloadedImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl PreloadedImageData {
    /// Converts raw rgba pixmap source to the rgba source with blended semi transparent colors.
    pub fn blend_rgba_slice(rgba_slice: &[u8]) -> Vec<u8> {
        let mut data = vec![0; rgba_slice.len()];

        for i in (0..rgba_slice.len()).step_by(4) {
            let r = rgba_slice[i];
            let g = rgba_slice[i + 1];
            let b = rgba_slice[i + 2];
            let a = rgba_slice[i + 3];

            let alpha = a as f32 / 255.0;

            data[i] = (r as f32 * alpha + 0.5) as u8;
            data[i + 1] = (g as f32 * alpha + 0.5) as u8;
            data[i + 2] = (b as f32 * alpha + 0.5) as u8;
            data[i + 3] = a;
        }

        data
    }

    /// Creates a new `PreloadedImageData` from the given rgba8 buffer and blends all the semi transparent colors.
    pub fn new(id: String, width: u32, height: u32, rgba_data: &[u8]) -> Self {
        Self {
            id,
            data: Cow::Owned(Self::blend_rgba_slice(rgba_data)),
            width,
            height,
        }
    }

    /// Creates a new `PreloadedImageData` from the given rgba8 buffer
    /// which is meant to be already blended for semi transparent colors.
    ///
    /// You can use `PreloadedImageData::blend_rgba_slice` to blend the colors in advance.
    pub fn new_blended(id: String, width: u32, height: u32, rgba_data: &'static [u8]) -> Self {
        Self {
            id,
            data: Cow::Borrowed(rgba_data),
            width,
            height,
        }
    }
}

pub(crate) fn convert(node: SvgNode, state: &converter::State, parent: &mut Group) -> Option<()> {
    let attr = node
        .attributes()
        .iter()
        .find(|a| a.name == AId::Href)
        .log_none(|| log::warn!("Image lacks the 'xlink:href' attribute. Skipped."))?;

    let (href, kind) = match attr.value.as_ref() {
        SvgAttributeValueRef::Str(href) => Some((href, get_href_data(href, state)?)),
        SvgAttributeValueRef::ImageData(image_data) => Some((
            image_data.id.as_str(),
            ImageKind::DATA(Arc::clone(image_data)),
        )),
        _ => None,
    }?;

    let visibility = node.find_attribute(AId::Visibility).unwrap_or_default();
    let rendering_mode = node
        .find_attribute(AId::ImageRendering)
        .unwrap_or(state.opt.image_rendering);

    let actual_size = match kind {
        ImageKind::DATA(ref data) => Size::from_wh(data.width as f32, data.height as f32)?,
        ImageKind::SVG { ref tree, .. } => tree.size,
    };

    let x = node.convert_user_length(AId::X, state, Length::zero());
    let y = node.convert_user_length(AId::Y, state, Length::zero());
    let mut width = node.convert_user_length(
        AId::Width,
        state,
        Length::new_number(actual_size.width() as f64),
    );
    let mut height = node.convert_user_length(
        AId::Height,
        state,
        Length::new_number(actual_size.height() as f64),
    );

    match (
        node.attribute::<Length>(AId::Width),
        node.attribute::<Length>(AId::Height),
    ) {
        (Some(_), None) => {
            // Only width was defined, so we need to scale height accordingly.
            height = actual_size.height() * (width / actual_size.width());
        }
        (None, Some(_)) => {
            // Only height was defined, so we need to scale width accordingly.
            width = actual_size.width() * (height / actual_size.height());
        }
        _ => {}
    };

    let rect = NonZeroRect::from_xywh(x, y, width, height);
    let rect = rect.log_none(|| log::warn!("Image has an invalid size. Skipped."))?;

    let view_box = ViewBox {
        rect,
        aspect: node.attribute(AId::PreserveAspectRatio).unwrap_or_default(),
    };

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let id = if state.parent_markers.is_empty() {
        node.element_id().to_string()
    } else {
        String::new()
    };

    let abs_bounding_box = view_box.rect.transform(parent.abs_transform)?;

    parent.children.push(Node::Image(Box::new(Image {
        origin_href: href.to_string(),
        id,
        visibility,
        view_box,
        rendering_mode,
        kind,
        abs_transform: parent.abs_transform,
        abs_bounding_box,
    })));

    Some(())
}

pub(crate) fn get_href_data(href: &str, state: &converter::State) -> Option<ImageKind> {
    let preloaded_image = state.opt.image_data?.get(href).map(Arc::clone);
    if let Some(data) = preloaded_image {
        return Some(ImageKind::DATA(data));
    }

    if let Some(sub_svg) = state.opt.sub_svg_data?.get(href) {
        return Some(ImageKind::SVG {
            tree: Arc::clone(sub_svg),
            original_href: href.to_string(),
        });
    }

    return None;
}
