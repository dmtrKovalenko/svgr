// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::convert::TryFrom;
use std::rc::Rc;
use std::str::FromStr;
use std::{collections::HashMap, ops::Range};

use super::{AId, Attribute, AttributeValue, Document, EId, Node, NodeData, NodeId, NodeKind};
use crate::svgtree::NestedNodeKind;
use crate::{
    svgtree::{NestedNodeData, NestedSvgDocument},
    EnableBackground, Error, Opacity, PathData, Rect,
};

pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NAMESPACE_NS: &str = "http://www.w3.org/XML/1998/namespace";

impl Document {
    pub fn parse(xml: &roxmltree::Document) -> Result<Document, Error> {
        parse(xml)
    }

    pub(super) fn append(&mut self, parent_id: NodeId, kind: NodeKind) -> NodeId {
        let new_child_id = NodeId(self.nodes.len());
        self.nodes.push(NodeData {
            parent: Some(parent_id),
            next_sibling: None,
            children: None,
            kind,
        });

        let last_child_id = self.nodes[parent_id.0].children.map(|(_, id)| id);

        if let Some(id) = last_child_id {
            self.nodes[id.0].next_sibling = Some(new_child_id);
        }

        self.nodes[parent_id.0].children = Some(
            if let Some((first_child_id, _)) = self.nodes[parent_id.0].children {
                (first_child_id, new_child_id)
            } else {
                (new_child_id, new_child_id)
            },
        );

        new_child_id
    }

    fn append_attribute(&mut self, tag_name: EId, aid: AId, value: &str) {
        let value2 = parse_svg_attribute(tag_name, aid, value);
        // TODO: improve error logging
        if let Some(value) = value2 {
            self.attrs.push(Attribute { name: aid, value });
        } else {
            // Invalid `enable-background` is not an error
            // since we are ignoring the `accumulate` value.
            if aid != AId::EnableBackground {
                log::warn!("Failed to parse {} value: '{}'.", aid, value);
            }
        }
    }

    /// Parses and appends or replaces an attribute
    fn insert_attribute(
        &mut self,
        aid: AId,
        value: &str,
        attrs_start_idx: usize,
        parent_id: NodeId,
        tag_name: EId,
    ) {
        // Check that attribute already exists.
        let idx = &self.attrs[attrs_start_idx..]
            .iter_mut()
            .position(|a| a.name == aid);

        // Append an attribute as usual.
        let added = append_attribute(parent_id, tag_name, aid, value, self);

        // Check that attribute was actually added, because it could be skipped.
        if added {
            if let Some(idx) = idx {
                // Swap the last attribute with an existing one.
                let last_idx = self.attrs.len() - 1;
                self.attrs.swap(attrs_start_idx + idx, last_idx);
                // Remove last.
                self.attrs.pop();
            }
        }
    }
}

fn prepare_raw_svgtree(doc: &mut Document) -> Result<(), Error> {
    // Check that the root element is `svg`.
    match doc.root().first_element_child() {
        Some(child) => {
            if child.tag_name() != Some(EId::Svg) {
                return Err(roxmltree::Error::NoRootNode.into());
            }
        }
        None => return Err(roxmltree::Error::NoRootNode.into()),
    }

    // Collect all elements with `id` attribute.
    let mut links = HashMap::new();
    for node in doc.descendants() {
        if let Some(id) = node.attribute::<&str>(AId::Id) {
            links.insert(id.to_string(), node.id);
        }
    }

    doc.links = links;
    fix_recursive_patterns(doc);
    fix_recursive_links(EId::ClipPath, AId::ClipPath, doc);
    fix_recursive_links(EId::Mask, AId::Mask, doc);
    fix_recursive_links(EId::Filter, AId::Filter, doc);
    fix_recursive_fe_image(doc);

    Ok(())
}

fn parse(xml: &roxmltree::Document) -> Result<Document, Error> {
    let mut doc = Document {
        nodes: Vec::new(),
        attrs: Vec::new(),
        links: HashMap::new(),
    };

    // Add a root node.
    doc.nodes.push(NodeData {
        parent: None,
        next_sibling: None,
        children: None,
        kind: NodeKind::Root,
    });

    let style_sheet = resolve_css(xml);

    parse_xml_node_children(
        xml.root(),
        xml.root(),
        doc.root().id,
        &style_sheet,
        false,
        0,
        &mut doc,
    )?;

    prepare_raw_svgtree(&mut doc)?;

    Ok(doc)
}

impl TryFrom<NestedSvgDocument> for Document {
    type Error = Error;

    fn try_from(nested_doc: NestedSvgDocument) -> Result<Self, Self::Error> {
        let mut doc = Document {
            nodes: Vec::new(),
            attrs: Vec::new(),
            links: HashMap::new(),
        };

        // Add a root node.
        doc.nodes.push(NodeData {
            parent: None,
            next_sibling: None,
            children: None,
            kind: NodeKind::Root,
        });

        let parent_id = doc.root().id;
        flatten_nested_svg_tree(&mut doc, &nested_doc, parent_id, &nested_doc.nodes);

        prepare_raw_svgtree(&mut doc)?;
        Ok(doc)
    }
}

fn flatten_nested_svg_tree(
    doc: &mut Document,
    nested_doc: &NestedSvgDocument,
    parent_id: NodeId,
    nodes: &[Option<NestedNodeData>],
) {
    for node in nodes.iter().flatten() {
        match &node.kind {
            NestedNodeKind::Element { tag_name, .. } => {
                append_nested_element(doc, nested_doc, node, node, parent_id, *tag_name, false);
            }
            NestedNodeKind::Text(value) => {
                doc.append(parent_id, NodeKind::Text(value.to_string()));
            }
            NestedNodeKind::Root => {
                let parent_id = doc.append(parent_id, NodeKind::Root);
                flatten_nested_svg_tree(doc, nested_doc, parent_id, &node.children)
            }
        };
    }
}

fn append_nested_element(
    doc: &mut Document,
    nested_doc: &NestedSvgDocument,
    node: &NestedNodeData,
    use_origin: &NestedNodeData,
    parent_id: NodeId,
    tag_name: EId,
    ignore_ids: bool,
) {
    let attrs_start_idx = doc.attrs.len();
    for attr in node.attrs.iter().flatten() {
        if let (AId::Style, AttributeValue::String(style)) = (&attr.name, &attr.value) {
            for declaration in simplecss::DeclarationTokenizer::from(style.as_str()) {
                if let Some(aid) = AId::from_str(declaration.name) {
                    // Parse only the presentation attributes.
                    if aid.is_presentation() {
                        doc.insert_attribute(
                            aid,
                            declaration.value,
                            attrs_start_idx,
                            parent_id,
                            tag_name,
                        );
                    }
                }
            }

            continue;
        }

        if attr.name == AId::Id && ignore_ids {
            continue;
        }

        doc.attrs.push(attr.clone());
    }

    let attributes = Range {
        start: attrs_start_idx,
        end: doc.attrs.len(),
    };

    if tag_name == EId::Use {
        let attrs_clone = attributes.clone();
        let node_id = doc.append(
            parent_id,
            NodeKind::Element {
                tag_name,
                attributes,
            },
        );

        resolve_nested_use_element(doc, nested_doc, node_id, node, use_origin, attrs_clone);
    } else {
        let node_id = doc.append(
            parent_id,
            NodeKind::Element {
                tag_name,
                attributes,
            },
        );

        flatten_nested_svg_tree(doc, nested_doc, node_id, &node.children)
    }
}
fn resolve_linked_node<'a>(
    href: &Attribute,
    nested_doc: &'a NestedSvgDocument,
) -> Option<&'a NestedNodeData> {
    let link_id = match &href.value {
        AttributeValue::Link(id) => id.as_str(),
        _ => return None,
    };

    let link_node = nested_doc
        .nodes
        .first()?
        .as_ref()?
        .find_recursively(&|node| {
            node.attrs.iter().flatten().any(|attr| {
                attr.name == AId::Id && attr.value == AttributeValue::String(link_id.to_string())
            })
        })?;

    Some(link_node)
}

fn resolve_nested_use_element(
    doc: &mut Document,
    nested_doc: &NestedSvgDocument,
    parent_id: NodeId,
    parent_node: &NestedNodeData,
    origin: &NestedNodeData,
    attributes_range: Range<usize>,
) -> Option<()> {
    let href = doc.attrs[attributes_range]
        .iter()
        .find(|attr| attr.name == AId::Href)?;
    let link_node = resolve_linked_node(href, nested_doc)?;

    // prevents stack overflow for recursive use elements
    if link_node == parent_node || link_node == origin {
        return None;
    };

    let is_recursive = link_node
        .find_recursively(&|node| match node.kind {
            NestedNodeKind::Element { tag_name } if tag_name == EId::Use => {
                let link2 = node
                    .attrs
                    .iter()
                    .flatten()
                    .find(|attr| attr.name == AId::Href);

                if let Some(href2) = link2 {
                    if let Some(link_node2) = resolve_linked_node(href2, nested_doc) {
                        return link_node2 == parent_node || link_node2 == link_node;
                    }
                }

                false
            }
            _ => false,
        })
        .is_some();

    if is_recursive {
        return None;
    }

    match link_node.kind {
        NestedNodeKind::Element { tag_name, .. } => {
            append_nested_element(
                doc,
                nested_doc,
                link_node,
                parent_node,
                parent_id,
                tag_name,
                true,
            );
            Some(())
        }
        _ => None,
    }
}

pub(super) fn parse_tag_name(node: roxmltree::Node) -> Option<EId> {
    if !node.is_element() {
        return None;
    }

    if node.tag_name().namespace() != Some(SVG_NS) {
        return None;
    }

    EId::from_str(node.tag_name().name())
}

fn parse_xml_node_children(
    parent: roxmltree::Node,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &simplecss::StyleSheet,
    ignore_ids: bool,
    depth: u32,
    doc: &mut Document,
) -> Result<(), Error> {
    for node in parent.children() {
        parse_xml_node(node, origin, parent_id, style_sheet, ignore_ids, depth, doc)?;
    }

    Ok(())
}

fn parse_xml_node(
    node: roxmltree::Node,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &simplecss::StyleSheet,
    ignore_ids: bool,
    depth: u32,
    doc: &mut Document,
) -> Result<(), Error> {
    if depth > 1024 {
        return Err(Error::ElementsLimitReached);
    }

    let mut tag_name = match parse_tag_name(node) {
        Some(id) => id,
        None => return Ok(()),
    };

    if tag_name == EId::Style {
        return Ok(());
    }

    // Treat links as groups.
    if tag_name == EId::A {
        tag_name = EId::G;
    }

    let node_id = parse_svg_element(node, parent_id, tag_name, style_sheet, ignore_ids, doc)?;
    if tag_name == EId::Text {
        super::text::parse_svg_text_element(node, node_id, style_sheet, doc)?;
    } else if tag_name == EId::Use {
        parse_svg_use_element(node, origin, node_id, style_sheet, depth + 1, doc)?;
    } else {
        parse_xml_node_children(
            node,
            origin,
            node_id,
            style_sheet,
            ignore_ids,
            depth + 1,
            doc,
        )?;
    }

    Ok(())
}

pub(super) fn parse_svg_element(
    xml_node: roxmltree::Node,
    parent_id: NodeId,
    tag_name: EId,
    style_sheet: &simplecss::StyleSheet,
    ignore_ids: bool,
    doc: &mut Document,
) -> Result<NodeId, Error> {
    let attrs_start_idx = doc.attrs.len();

    // Copy presentational attributes first.
    for attr in xml_node.attributes() {
        match attr.namespace() {
            None | Some(SVG_NS) | Some(XLINK_NS) | Some(XML_NAMESPACE_NS) => {}
            _ => continue,
        }

        let aid = match AId::from_str(attr.name()) {
            Some(v) => v,
            None => continue,
        };

        // During a `use` resolving, all `id` attributes must be ignored.
        // Otherwise we will get elements with duplicated id's.
        if ignore_ids && aid == AId::Id {
            continue;
        }

        // For some reason those properties are allowed only inside a `style` attribute and CSS.
        if matches!(aid, AId::MixBlendMode | AId::Isolation | AId::FontKerning) {
            continue;
        }

        append_attribute(parent_id, tag_name, aid, attr.value(), doc);
    }

    // Apply CSS.
    for rule in &style_sheet.rules {
        if rule.selector.matches(&XmlNode(xml_node)) {
            for declaration in &rule.declarations {
                // TODO: perform XML attribute normalization
                if let Some(aid) = AId::from_str(declaration.name) {
                    // Parse only the presentation attributes.
                    if aid.is_presentation() {
                        doc.insert_attribute(
                            aid,
                            declaration.value,
                            attrs_start_idx,
                            parent_id,
                            tag_name,
                        );
                    }
                } else if declaration.name == "marker" {
                    doc.insert_attribute(
                        AId::MarkerStart,
                        declaration.value,
                        attrs_start_idx,
                        parent_id,
                        tag_name,
                    );
                    doc.insert_attribute(
                        AId::MarkerMid,
                        declaration.value,
                        attrs_start_idx,
                        parent_id,
                        tag_name,
                    );
                    doc.insert_attribute(
                        AId::MarkerEnd,
                        declaration.value,
                        attrs_start_idx,
                        parent_id,
                        tag_name,
                    );
                }
            }
        }
    }

    // Split a `style` attribute.
    if let Some(value) = xml_node.attribute("style") {
        for declaration in simplecss::DeclarationTokenizer::from(value) {
            // TODO: preform XML attribute normalization
            if let Some(aid) = AId::from_str(declaration.name) {
                // Parse only the presentation attributes.
                if aid.is_presentation() {
                    doc.insert_attribute(
                        aid,
                        declaration.value,
                        attrs_start_idx,
                        parent_id,
                        tag_name,
                    );
                }
            }
        }
    }

    if doc.nodes.len() > 1_000_000 {
        return Err(Error::ElementsLimitReached);
    }

    let node_id = doc.append(
        parent_id,
        NodeKind::Element {
            tag_name,
            attributes: attrs_start_idx..doc.attrs.len(),
        },
    );

    Ok(node_id)
}

fn append_attribute(
    parent_id: NodeId,
    tag_name: EId,
    aid: AId,
    value: &str,
    doc: &mut Document,
) -> bool {
    match aid {
        // The `style` attribute will be split into attributes, so we don't need it.
        AId::Style |
        // No need to copy a `class` attribute since CSS were already resolved.
        AId::Class => return false,
        _ => {}
    }

    // Ignore `xlink:href` on `tspan` (which was originally `tref` or `a`),
    // because we will convert `tref` into `tspan` anyway.
    if tag_name == EId::Tspan && aid == AId::Href {
        return false;
    }

    if aid.allows_inherit_value() && value == "inherit" {
        return resolve_inherit(parent_id, tag_name, aid, doc);
    }

    doc.append_attribute(tag_name, aid, value);
    true
}

pub fn parse_svg_attribute(tag_name: EId, aid: AId, value: &str) -> Option<AttributeValue> {
    Some(match aid {
        AId::Href => {
            // `href` can contain base64 data and we do store it as is.
            match svgrtypes::IRI::from_str(value) {
                Ok(link) => AttributeValue::Link(link.0.to_string()),
                // TODO: avoid string allocation
                Err(_) => AttributeValue::String(value.to_string()),
            }
        }

        AId::X | AId::Y | AId::Dx | AId::Dy => {
            // Some attributes can contain different data based on the element type.
            match tag_name {
                EId::Text | EId::Tref | EId::Tspan => AttributeValue::String(value.to_string()),
                EId::FePointLight | EId::FeSpotLight | EId::FeOffset | EId::FeDropShadow => {
                    AttributeValue::Number(svgrtypes::Number::from_str(value).ok()?.0)
                }
                _ => AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?),
            }
        }

        AId::X1
        | AId::Y1
        | AId::X2
        | AId::Y2
        | AId::R
        | AId::Rx
        | AId::Ry
        | AId::Cx
        | AId::Cy
        | AId::Fx
        | AId::Fy
        | AId::RefX
        | AId::RefY
        | AId::Kerning
        | AId::MarkerWidth
        | AId::MarkerHeight
        | AId::StartOffset
        | AId::TextLength => AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?),

        AId::Width | AId::Height => {
            if value != "auto" {
                AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?)
            } else {
                // For now, we simply treat the `auto` value as `none`.
                // Since the resolving logic for `auto` is the same as no attribute.
                AttributeValue::None
            }
        }

        AId::Offset => {
            if let EId::FeFuncR | EId::FeFuncG | EId::FeFuncB | EId::FeFuncA = tag_name {
                AttributeValue::Number(svgrtypes::Number::from_str(value).ok()?.0)
            } else {
                // offset = <number> | <percentage>
                let l = svgrtypes::Length::from_str(value).ok()?;
                if l.unit == svgrtypes::LengthUnit::None || l.unit == svgrtypes::LengthUnit::Percent
                {
                    AttributeValue::Length(l)
                } else {
                    return None;
                }
            }
        }

        AId::StrokeDashoffset | AId::StrokeWidth => {
            AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?)
        }

        AId::Opacity
        | AId::FillOpacity
        | AId::FloodOpacity
        | AId::StrokeOpacity
        | AId::StopOpacity => {
            let length = svgrtypes::Length::from_str(value).ok()?;
            if length.unit == svgrtypes::LengthUnit::Percent {
                AttributeValue::Opacity(Opacity::new_clamped(length.number / 100.0))
            } else if length.unit == svgrtypes::LengthUnit::None {
                AttributeValue::Opacity(Opacity::new_clamped(length.number))
            } else {
                return None;
            }
        }

        AId::Amplitude
        | AId::Azimuth
        | AId::Bias
        | AId::DiffuseConstant
        | AId::Divisor
        | AId::Elevation
        | AId::Exponent
        | AId::Intercept
        | AId::K1
        | AId::K2
        | AId::K3
        | AId::K4
        | AId::LimitingConeAngle
        | AId::NumOctaves
        | AId::PointsAtX
        | AId::PointsAtY
        | AId::PointsAtZ
        | AId::Scale
        | AId::Seed
        | AId::Slope
        | AId::SpecularConstant
        | AId::SpecularExponent
        | AId::StrokeMiterlimit
        | AId::SurfaceScale
        | AId::TargetX
        | AId::TargetY
        | AId::Z => AttributeValue::Number(svgrtypes::Number::from_str(value).ok()?.0),

        AId::StrokeDasharray => match value {
            "none" => AttributeValue::None,
            _ => AttributeValue::String(value.to_string()),
        },

        AId::Fill => match svgrtypes::Paint::from_str(value) {
            Ok(svgrtypes::Paint::None) => AttributeValue::None,
            Ok(svgrtypes::Paint::Inherit) => unreachable!(),
            Ok(svgrtypes::Paint::CurrentColor) => AttributeValue::CurrentColor,
            Ok(svgrtypes::Paint::Color(color)) => AttributeValue::Color(color),
            Ok(svgrtypes::Paint::FuncIRI(link, fallback)) => {
                AttributeValue::Paint(link.to_string(), fallback)
            }
            Err(_) => {
                log::warn!(
                    "Failed to parse fill value: '{}'. Fallback to black.",
                    value
                );
                AttributeValue::Color(svgrtypes::Color::black())
            }
        },

        AId::Stroke => match svgrtypes::Paint::from_str(value).ok()? {
            svgrtypes::Paint::None => AttributeValue::None,
            svgrtypes::Paint::Inherit => unreachable!(),
            svgrtypes::Paint::CurrentColor => AttributeValue::CurrentColor,
            svgrtypes::Paint::Color(color) => AttributeValue::Color(color),
            svgrtypes::Paint::FuncIRI(link, fallback) => {
                AttributeValue::Paint(link.to_string(), fallback)
            }
        },

        AId::ClipPath | AId::MarkerEnd | AId::MarkerMid | AId::MarkerStart | AId::Mask => {
            match value {
                "none" => AttributeValue::None,
                _ => {
                    let link = svgrtypes::FuncIRI::from_str(value).ok()?;
                    AttributeValue::Link(link.0.to_string())
                }
            }
        }

        AId::Color => AttributeValue::Color(svgrtypes::Color::from_str(value).ok()?),

        AId::FloodColor | AId::LightingColor | AId::StopColor => match value {
            "currentColor" => AttributeValue::CurrentColor,
            _ => AttributeValue::Color(svgrtypes::Color::from_str(value).ok()?),
        },

        AId::D => {
            let mut path = PathData::new();
            for segment in svgrtypes::SimplifyingPathParser::from(value) {
                let segment = match segment {
                    Ok(v) => v,
                    Err(_) => break,
                };

                match segment {
                    svgrtypes::SimplePathSegment::MoveTo { x, y } => {
                        path.push_move_to(x, y);
                    }
                    svgrtypes::SimplePathSegment::LineTo { x, y } => {
                        path.push_line_to(x, y);
                    }
                    svgrtypes::SimplePathSegment::CurveTo {
                        x1,
                        y1,
                        x2,
                        y2,
                        x,
                        y,
                    } => {
                        path.push_curve_to(x1, y1, x2, y2, x, y);
                    }
                    svgrtypes::SimplePathSegment::Quadratic { x1, y1, x, y } => {
                        path.push_quad_to(x1, y1, x, y);
                    }
                    svgrtypes::SimplePathSegment::ClosePath => {
                        path.push_close_path();
                    }
                }
            }

            if path.len() >= 2 {
                AttributeValue::Path(Rc::new(path))
            } else {
                return None;
            }
        }

        AId::Transform | AId::GradientTransform | AId::PatternTransform => {
            AttributeValue::Transform(svgrtypes::Transform::from_str(value).ok()?.into())
        }

        AId::TransformOrigin => {
            AttributeValue::TransformOrigin(svgrtypes::TransformOrigin::from_str(value).ok()?)
        }

        // | AId::TransformOrigin => {
        //     AttributeValue::TransformOrigin(crate::TransformOrigin::from_str(value).ok()?.into())
        // }
        AId::FontSize => match svgrtypes::Length::from_str(value) {
            Ok(l) => AttributeValue::Length(l),
            Err(_) => AttributeValue::String(value.to_string()),
        },

        AId::Display | AId::TextDecoration => match value {
            "none" => AttributeValue::None,
            _ => AttributeValue::String(value.to_string()),
        },
        AId::LetterSpacing | AId::WordSpacing => match value {
            "normal" => AttributeValue::String(value.to_string()),
            _ => AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?),
        },

        AId::BaselineShift => match value {
            "baseline" | "sub" | "super" => AttributeValue::String(value.to_string()),
            _ => AttributeValue::Length(svgrtypes::Length::from_str(value).ok()?),
        },

        AId::Orient => match value {
            "auto" => AttributeValue::String(value.to_string()),
            _ => AttributeValue::Angle(svgrtypes::Angle::from_str(value).ok()?),
        },

        AId::ViewBox => AttributeValue::ViewBox(svgrtypes::ViewBox::from_str(value).ok()?),

        AId::PreserveAspectRatio => {
            AttributeValue::AspectRatio(svgrtypes::AspectRatio::from_str(value).ok()?)
        }

        AId::PaintOrder => AttributeValue::PaintOrder(svgrtypes::PaintOrder::from_str(value).ok()?),

        AId::BaseFrequency
        | AId::KernelMatrix
        | AId::Radius
        | AId::Rotate
        | AId::TableValues
        | AId::Values => {
            let mut numbers = Vec::new();
            for n in svgrtypes::NumberListParser::from(value) {
                numbers.push(n.ok()?);
            }

            AttributeValue::NumberList(numbers)
        }

        AId::EnableBackground => {
            let eb = svgrtypes::EnableBackground::from_str(value).ok()?;
            match eb {
                svgrtypes::EnableBackground::Accumulate => return None,
                svgrtypes::EnableBackground::New => {
                    AttributeValue::EnableBackground(EnableBackground(None))
                }
                svgrtypes::EnableBackground::NewWithRegion {
                    x,
                    y,
                    width,
                    height,
                } => {
                    let r = Rect::new(x, y, width, height)?;
                    AttributeValue::EnableBackground(EnableBackground(Some(r)))
                }
            }
        }

        _ => AttributeValue::String(value.to_string()),
    })
}

fn resolve_inherit(parent_id: NodeId, tag_name: EId, aid: AId, doc: &mut Document) -> bool {
    if aid.is_inheritable() {
        // Inheritable attributes can inherit a value from an any ancestor.
        let node_id = doc
            .get(parent_id)
            .find_node_with_attribute(aid)
            .map(|n| n.id);
        if let Some(node_id) = node_id {
            if let Some(attr) = doc
                .get(node_id)
                .attributes()
                .iter()
                .find(|a| a.name == aid)
                .cloned()
            {
                doc.attrs.push(Attribute {
                    name: aid,
                    value: attr.value,
                });

                return true;
            }
        }
    } else {
        // Non-inheritable attributes can inherit a value only from a direct parent.
        if let Some(attr) = doc
            .get(parent_id)
            .attributes()
            .iter()
            .find(|a| a.name == aid)
            .cloned()
        {
            doc.attrs.push(Attribute {
                name: aid,
                value: attr.value,
            });

            return true;
        }
    }

    // Fallback to a default value if possible.
    let value = match aid {
        AId::ImageRendering | AId::ShapeRendering | AId::TextRendering => "auto",

        AId::ClipPath
        | AId::Filter
        | AId::MarkerEnd
        | AId::MarkerMid
        | AId::MarkerStart
        | AId::Mask
        | AId::Stroke
        | AId::StrokeDasharray
        | AId::TextDecoration => "none",

        AId::FontStretch
        | AId::FontStyle
        | AId::FontVariant
        | AId::FontWeight
        | AId::LetterSpacing
        | AId::WordSpacing => "normal",

        AId::Fill | AId::FloodColor | AId::StopColor => "black",

        AId::FillOpacity
        | AId::FloodOpacity
        | AId::Opacity
        | AId::StopOpacity
        | AId::StrokeOpacity => "1",

        AId::ClipRule | AId::FillRule => "nonzero",

        AId::BaselineShift => "baseline",
        AId::ColorInterpolationFilters => "linearRGB",
        AId::Direction => "ltr",
        AId::Display => "inline",
        AId::FontSize => "medium",
        AId::Overflow => "visible",
        AId::StrokeDashoffset => "0",
        AId::StrokeLinecap => "butt",
        AId::StrokeLinejoin => "miter",
        AId::StrokeMiterlimit => "4",
        AId::StrokeWidth => "1",
        AId::TextAnchor => "start",
        AId::Visibility => "visible",
        AId::WritingMode => "lr-tb",
        _ => return false,
    };

    doc.append_attribute(tag_name, aid, value);
    true
}

fn resolve_href<'a>(node: roxmltree::Node<'a, 'a>) -> Option<roxmltree::Node<'a, 'a>> {
    let link_value = node
        .attribute((XLINK_NS, "href"))
        .or_else(|| node.attribute("href"))?;

    let link_id = svgrtypes::IRI::from_str(link_value).ok()?.0;

    // We're using `descendants` each time instead of HashTable because
    // we have to preserve the original elements order.
    // See tests/svg/e-use-024.svg
    //
    // Technically we can use https://crates.io/crates/hashlink,
    // but this is an additional dependency.
    // And performance even on huge files is still good enough.
    node.document()
        .descendants()
        .find(|n| n.attribute("id") == Some(link_id))
}

fn parse_svg_use_element(
    node: roxmltree::Node,
    origin: roxmltree::Node,
    parent_id: NodeId,
    style_sheet: &simplecss::StyleSheet,
    depth: u32,
    doc: &mut Document,
) -> Result<(), Error> {
    let link = match resolve_href(node) {
        Some(v) => v,
        None => return Ok(()),
    };

    if link == node || link == origin {
        log::warn!(
            "Recursive 'use' detected. '{}' will be skipped.",
            node.attribute((SVG_NS, "id")).unwrap_or_default()
        );
        return Ok(());
    }

    // Make sure we're linked to an SVG element.
    if parse_tag_name(link).is_none() {
        return Ok(());
    }

    // Check that none of the linked node's children reference current `use` node
    // via other `use` node.
    //
    // Example:
    // <g id="g1">
    //     <use xlink:href="#use1" id="use2"/>
    // </g>
    // <use xlink:href="#g1" id="use1"/>
    //
    // `use2` should be removed.
    //
    // Also, child should not reference its parent:
    // <g id="g1">
    //     <use xlink:href="#g1" id="use1"/>
    // </g>
    //
    // `use1` should be removed.
    let mut is_recursive = false;
    for link_child in link
        .descendants()
        .skip(1)
        .filter(|n| n.has_tag_name((SVG_NS, "use")))
    {
        if let Some(link2) = resolve_href(link_child) {
            if link2 == node || link2 == link {
                is_recursive = true;
                break;
            }
        }
    }

    if is_recursive {
        log::warn!(
            "Recursive 'use' detected. '{}' will be skipped.",
            node.attribute((SVG_NS, "id")).unwrap_or_default()
        );
        return Ok(());
    }

    parse_xml_node(link, node, parent_id, style_sheet, true, depth + 1, doc)
}

fn resolve_css<'a>(xml: &'a roxmltree::Document<'a>) -> simplecss::StyleSheet<'a> {
    let mut sheet = simplecss::StyleSheet::new();

    for node in xml.descendants().filter(|n| n.has_tag_name("style")) {
        match node.attribute("type") {
            Some("text/css") => {}
            Some(_) => continue,
            None => {}
        }

        let text = match node.text() {
            Some(v) => v,
            None => continue,
        };

        sheet.parse_more(text);
    }

    sheet
}

struct XmlNode<'a, 'input: 'a>(roxmltree::Node<'a, 'input>);

impl simplecss::Element for XmlNode<'_, '_> {
    fn parent_element(&self) -> Option<Self> {
        self.0.parent_element().map(XmlNode)
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.0.prev_sibling_element().map(XmlNode)
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.0.tag_name().name() == local_name
    }

    fn attribute_matches(&self, local_name: &str, operator: simplecss::AttributeOperator) -> bool {
        match self.0.attribute(local_name) {
            Some(value) => operator.matches(value),
            None => false,
        }
    }

    fn pseudo_class_matches(&self, class: simplecss::PseudoClass) -> bool {
        match class {
            simplecss::PseudoClass::FirstChild => self.prev_sibling_element().is_none(),
            // TODO: lang
            _ => false, // Since we are querying a static SVG we can ignore other pseudo-classes.
        }
    }
}

fn fix_recursive_patterns(doc: &mut Document) {
    while let Some(node_id) = find_recursive_pattern(AId::Fill, doc) {
        let idx = doc.get(node_id).attribute_id(AId::Fill).unwrap();
        doc.attrs[idx.0].value = AttributeValue::None;
    }

    while let Some(node_id) = find_recursive_pattern(AId::Stroke, doc) {
        let idx = doc.get(node_id).attribute_id(AId::Stroke).unwrap();
        doc.attrs[idx.0].value = AttributeValue::None;
    }
}

fn find_recursive_pattern(aid: AId, doc: &mut Document) -> Option<NodeId> {
    for pattern_node in doc
        .root()
        .descendants()
        .filter(|n| n.has_tag_name(EId::Pattern))
    {
        for node in pattern_node.descendants() {
            if let Some(&AttributeValue::Paint(ref link_id, _)) = node.attribute(aid) {
                if link_id == pattern_node.element_id() {
                    // If a pattern child has a link to the pattern itself
                    // then we have to replace it with `none`.
                    // Otherwise we will get endless loop/recursion and stack overflow.
                    return Some(node.id);
                } else {
                    // Check that linked node children doesn't link this pattern.
                    if let Some(linked_node) = doc.element_by_id(link_id) {
                        for node2 in linked_node.descendants() {
                            if let Some(&AttributeValue::Paint(ref link_id2, _)) =
                                node2.attribute(aid)
                            {
                                if link_id2 == pattern_node.element_id() {
                                    return Some(node2.id);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn fix_recursive_links(eid: EId, aid: AId, doc: &mut Document) {
    while let Some(node_id) = find_recursive_link(eid, aid, doc) {
        let idx = doc.get(node_id).attribute_id(aid).unwrap();
        doc.attrs[idx.0].value = AttributeValue::None;
    }
}

fn find_recursive_link(eid: EId, aid: AId, doc: &Document) -> Option<NodeId> {
    for node in doc.root().descendants().filter(|n| n.has_tag_name(eid)) {
        for child in node.descendants() {
            if let Some(link) = child.attribute::<Node>(aid) {
                if link == node {
                    // If an element child has a link to the element itself
                    // then we have to replace it with `none`.
                    // Otherwise we will get endless loop/recursion and stack overflow.
                    return Some(child.id);
                } else {
                    // Check that linked node children doesn't link this element.
                    for node2 in link.descendants() {
                        if let Some(link2) = node2.attribute::<Node>(aid) {
                            if link2 == node {
                                return Some(node2.id);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detects cases like:
///
/// ```xml
/// <filter id="filter1">
///   <feImage xlink:href="#rect1"/>
/// </filter>
/// <rect id="rect1" x="36" y="36" width="120" height="120" fill="green" filter="url(#filter1)"/>
/// ```
fn fix_recursive_fe_image(doc: &mut Document) {
    let mut ids = Vec::new();
    for fe_node in doc
        .root()
        .descendants()
        .filter(|n| n.has_tag_name(EId::FeImage))
    {
        if let Some(link) = fe_node.attribute::<Node>(AId::Href) {
            if let Some(filter_uri) = link.attribute::<&str>(AId::Filter) {
                let filter_id = fe_node.parent().unwrap().element_id().to_string();
                for func in svgrtypes::FilterValueListParser::from(filter_uri) {
                    if let Ok(func) = func {
                        if let svgrtypes::FilterValue::Url(url) = func {
                            if url == filter_id {
                                ids.push(link.id);
                            }
                        }
                    }
                }
            }
        }
    }

    for id in ids {
        let idx = doc.get(id).attribute_id(AId::Filter).unwrap();
        doc.attrs[idx.0].value = AttributeValue::None;
    }
}
