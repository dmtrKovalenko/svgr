// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
#![allow(missing_docs)]
use std::num::NonZeroU32;
use std::str::FromStr;
use std::{collections::HashMap, sync::Arc};

#[rustfmt::skip] mod names;
/// FFrames: parser should be available publicly for the svg macro
pub mod parse;
mod text;

pub use roxmltree::StringStorage;
pub use svgrtypes;
use svgrtypes::LengthUnit;
use tiny_skia_path::Transform;

use crate::{
    BlendMode, ImageRendering, Opacity, PreloadedImageData, ShapeRendering, SpreadMethod,
    TextRendering, Units, Visibility,
};
pub use names::{AId, EId, ATTRIBUTES};
pub use roxmltree;

#[cfg(feature = "proc-macro")]
pub use self_rust_tokenize::*;

#[derive(Debug, Clone)]
/// FFrames specific composable svg document that flattens in runtime
pub struct NestedSvgDocument<'input, TNode = NestedNodeData<'input>> {
    /// Nodes of the SVG document.
    pub nodes: Vec<Option<TNode>>,
    marker: std::marker::PhantomData<&'input ()>,
}

impl<'input> NestedSvgDocument<'input> {
    /// Create new document from a vec of nodes
    pub fn from_nodes(nodes: Vec<Option<NestedNodeData<'input>>>) -> Self {
        Self {
            nodes,
            marker: std::marker::PhantomData,
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for NestedSvgDocument<'_> {
    fn default() -> Self {
        Self {
            nodes: vec![],
            marker: std::marker::PhantomData,
        }
    }
}

impl NestedNodeData<'_> {
    pub(crate) fn find_recursively(
        &self,
        predicate: &impl Fn(&NestedNodeData) -> bool,
    ) -> Option<&NestedNodeData> {
        for node in self.children.iter().flatten() {
            if predicate(node) {
                return Some(node);
            }

            if let Some(res) = node.find_recursively(predicate) {
                return Some(res);
            }
        }

        None
    }
}

/// Used for svg macro in FFramea
pub mod macro_prelude {
    pub use super::*;
    pub use roxmltree;
    pub use strict_num::NormalizedF64;
}

/// An SVG tree container.
///
/// Contains only element and text nodes.
/// Text nodes are present only inside the `text` element.
pub struct Document<'input> {
    nodes: Vec<NodeData>,
    attrs: Vec<Attribute<'input>>,
    links: HashMap<String, NodeId>,
}

impl<'input> Document<'input> {
    /// Returns the root node.
    #[inline]
    pub fn root<'a>(&'a self) -> SvgNode<'a, 'input> {
        SvgNode {
            id: NodeId::new(0),
            d: &self.nodes[0],
            doc: self,
        }
    }

    /// Returns the root element.
    #[inline]
    pub fn root_element<'a>(&'a self) -> SvgNode<'a, 'input> {
        // `unwrap` is safe, because `Document` is guarantee to have at least one element.
        self.root().first_element_child().unwrap()
    }

    /// Returns an iterator over document's descendant nodes.
    ///
    /// Shorthand for `doc.root().descendants()`.
    #[inline]
    pub fn descendants<'a>(&'a self) -> Descendants<'a, 'input> {
        self.root().descendants()
    }

    /// Returns an element by ID.
    ///
    /// Unlike the [`Descendants`] iterator, this is just a HashMap lookup.
    /// Meaning it's way faster.
    #[inline]
    pub fn element_by_id<'a>(&'a self, id: &str) -> Option<SvgNode<'a, 'input>> {
        let node_id = self.links.get(id)?;
        Some(self.get(*node_id))
    }

    #[inline]
    fn get<'a>(&'a self, id: NodeId) -> SvgNode<'a, 'input> {
        SvgNode {
            id,
            d: &self.nodes[id.get_usize()],
            doc: self,
        }
    }

    fn insert_attribute(
        &mut self,
        aid: AId,
        value: &'input str,
        attrs_start_idx: usize,
        parent_id: NodeId,
        tag_name: EId,
    ) {
        // Check that attribute already exists.
        let idx = &self.attrs[attrs_start_idx..]
            .iter_mut()
            .position(|a| a.name == aid);

        // Append an attribute as usual.
        let added = parse::append_attribute(
            parent_id,
            tag_name,
            aid,
            StringStorage::Borrowed(value),
            self,
        );

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

impl std::fmt::Debug for Document<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        if !self.root().has_children() {
            return write!(f, "Document []");
        }

        macro_rules! writeln_indented {
            ($depth:expr, $f:expr, $fmt:expr) => {
                for _ in 0..$depth { write!($f, "    ")?; }
                writeln!($f, $fmt)?;
            };
            ($depth:expr, $f:expr, $fmt:expr, $($arg:tt)*) => {
                for _ in 0..$depth { write!($f, "    ")?; }
                writeln!($f, $fmt, $($arg)*)?;
            };
        }

        fn print_children(
            parent: SvgNode,
            depth: usize,
            f: &mut std::fmt::Formatter,
        ) -> Result<(), std::fmt::Error> {
            for child in parent.children() {
                if child.is_element() {
                    writeln_indented!(depth, f, "Element {{");
                    writeln_indented!(depth, f, "    tag_name: {:?}", child.tag_name());

                    if !child.attributes().is_empty() {
                        writeln_indented!(depth + 1, f, "attributes: [");
                        for attr in child.attributes() {
                            writeln_indented!(depth + 2, f, "{:?}", attr);
                        }
                        writeln_indented!(depth + 1, f, "]");
                    }

                    if child.has_children() {
                        writeln_indented!(depth, f, "    children: [");
                        print_children(child, depth + 2, f)?;
                        writeln_indented!(depth, f, "    ]");
                    }

                    writeln_indented!(depth, f, "}}");
                } else {
                    writeln_indented!(depth, f, "{:?}", child);
                }
            }

            Ok(())
        }

        writeln!(f, "Document [")?;
        print_children(self.root(), 1, f)?;
        writeln!(f, "]")?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShortRange {
    start: u32,
    end: u32,
}

impl ShortRange {
    #[inline]
    fn new(start: u32, end: u32) -> Self {
        ShortRange { start, end }
    }

    #[inline]
    fn to_urange(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct NodeId(NonZeroU32);

impl NodeId {
    #[inline]
    fn new(id: u32) -> Self {
        debug_assert!(id < core::u32::MAX);

        // We are using `NonZeroU32` to reduce overhead of `Option<NodeId>`.
        NodeId(NonZeroU32::new(id + 1).unwrap())
    }

    #[inline]
    fn get(self) -> u32 {
        self.0.get() - 1
    }

    #[inline]
    fn get_usize(self) -> usize {
        self.get() as usize
    }
}

impl From<usize> for NodeId {
    #[inline]
    fn from(id: usize) -> Self {
        // We already checked that `id` is limited by u32::MAX.
        debug_assert!(id <= core::u32::MAX as usize);
        NodeId::new(id as u32)
    }
}

pub(crate) enum NodeKind {
    Root,
    Element {
        tag_name: EId,
        attributes: ShortRange,
    },
    Text(String),
}

struct NodeData {
    parent: Option<NodeId>,
    next_sibling: Option<NodeId>,
    children: Option<(NodeId, NodeId)>,
    kind: NodeKind,
}

#[derive(Debug, PartialEq, Clone)]
#[allow(missing_docs)]
/// FFrames change: NestedNodeKind used to construct trees in macro
pub enum NestedNodeKind<'a> {
    Root,
    Element { tag_name: EId },
    Text(roxmltree::StringStorage<'a>),
}

#[cfg(feature = "proc-macro")]
impl quote::ToTokens for NestedNodeKind<'_> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            NestedNodeKind::Root => quote! { NestedNodeKind::Root },
            NestedNodeKind::Element { tag_name } => {
                let tag_name = tag_name.to_tokens();
                quote! {
                    NestedNodeKind::Element {
                        tag_name: #tag_name,
                    }
                }
            }
            // the trick here: when we convert totokens we never need to own the string
            NestedNodeKind::Text(roxmltree::StringStorage::Owned(value)) => {
                use std::ops::Deref;
                let value = value.deref();
                quote! { NestedNodeKind::Text(roxmltree::StringStorage::Borrowed(#value)) }
            }
            NestedNodeKind::Text(roxmltree::StringStorage::Borrowed(value)) => {
                quote! { NestedNodeKind::Text(roxmltree::StringStorage::Borrowed(#value)) }
            }
        }
        .to_tokens(tokens)
    }
}

#[derive(Debug, PartialEq, Clone)]
#[allow(missing_docs)]
/// FFrames change: NestedNodeDataused to construct trees in macro
pub struct NestedNodeData<'input> {
    pub kind: NestedNodeKind<'input>,
    pub attrs: Vec<Attribute<'input>>,
    pub children: Vec<Option<NestedNodeData<'input>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SvgAttributeValue<'a> {
    Float(f32, StringStorage<'a>),
    Length(svgrtypes::Length),
    Transform(svgrtypes::Transform),
    Color(svgrtypes::Color),
    StringStorage(StringStorage<'a>),
    ImageData(Arc<PreloadedImageData>),
}

impl From<String> for SvgAttributeValue<'_> {
    fn from(s: String) -> Self {
        SvgAttributeValue::StringStorage(roxmltree::StringStorage::Owned(s.into()))
    }
}

impl From<Arc<PreloadedImageData>> for SvgAttributeValue<'_> {
    fn from(image: Arc<PreloadedImageData>) -> Self {
        SvgAttributeValue::ImageData(image)
    }
}

impl<'a> From<&'a str> for SvgAttributeValue<'a> {
    fn from(s: &'a str) -> Self {
        SvgAttributeValue::StringStorage(roxmltree::StringStorage::Borrowed(s))
    }
}

// I agree it looks strange but it actually happens quite a lot within
// fframes implementation so we just have it for DX reasons
impl<'a> From<&&'a str> for SvgAttributeValue<'a> {
    fn from(s: &&'a str) -> Self {
        SvgAttributeValue::StringStorage(roxmltree::StringStorage::Borrowed(*s))
    }
}

macro_rules! impl_value_from_numeric {
    ($target:ty) => {
        impl From<$target> for SvgAttributeValue<'_> {
            fn from(v: $target) -> Self {
                SvgAttributeValue::Float(
                    v as f32,
                    roxmltree::StringStorage::Owned(v.to_string().into()),
                )
            }
        }

        impl From<&$target> for SvgAttributeValue<'_> {
            fn from(v: &$target) -> Self {
                SvgAttributeValue::Float(
                    *v as f32,
                    roxmltree::StringStorage::Owned(v.to_string().into()),
                )
            }
        }
    };
}

impl_value_from_numeric!(f32);
impl_value_from_numeric!(f64);
impl_value_from_numeric!(i16);
impl_value_from_numeric!(i32);
impl_value_from_numeric!(i64);
impl_value_from_numeric!(i8);
impl_value_from_numeric!(isize);
impl_value_from_numeric!(u16);
impl_value_from_numeric!(u32);
impl_value_from_numeric!(u64);
impl_value_from_numeric!(u8);
impl_value_from_numeric!(usize);

impl From<svgrtypes::Length> for SvgAttributeValue<'_> {
    fn from(v: svgrtypes::Length) -> Self {
        SvgAttributeValue::Length(v)
    }
}

impl From<svgrtypes::Transform> for SvgAttributeValue<'_> {
    fn from(v: svgrtypes::Transform) -> Self {
        SvgAttributeValue::Transform(v)
    }
}

impl<'a> SvgAttributeValue<'a> {
    pub fn as_ref(&'a self) -> SvgAttributeValueRef<'a> {
        match self {
            SvgAttributeValue::StringStorage(s) => SvgAttributeValueRef::Str(s.as_ref()),
            SvgAttributeValue::Float(f, s) => SvgAttributeValueRef::Float(*f, s.as_ref()),
            SvgAttributeValue::Length(v) => SvgAttributeValueRef::Length(*v),
            SvgAttributeValue::Transform(v) => SvgAttributeValueRef::Transform(*v),
            SvgAttributeValue::Color(c) => SvgAttributeValueRef::Color(*c),
            SvgAttributeValue::ImageData(image) => SvgAttributeValueRef::ImageData(&image),
        }
    }
}

impl std::fmt::Display for SvgAttributeValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            SvgAttributeValue::StringStorage(s) => write!(f, "{}", s),
            SvgAttributeValue::Float(_, s) => write!(f, "{}", s),
            SvgAttributeValue::Length(v) => write!(f, "{:?}", v),
            SvgAttributeValue::Transform(v) => write!(f, "{:?}", v),
            // TODO figure out if it it as in issue that we ignore the alpha here
            SvgAttributeValue::Color(svgrtypes::Color {
                red, green, blue, ..
            }) => write!(f, "rgb({red}, {green}, {blue})"),
            SvgAttributeValue::ImageData(ref image) => write!(f, "{:?}", image.id),
        }
    }
}

/// An attribute.
#[derive(Clone, PartialEq)]
pub struct Attribute<'input> {
    /// Attribute's name.
    pub name: AId,
    /// Attribute's value.
    pub value: SvgAttributeValue<'input>,
}

impl std::fmt::Debug for Attribute<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Attribute {{ name: {:?}, value: {} }}",
            self.name, self.value
        )
    }
}

/// An SVG node.
#[derive(Clone, Copy)]
pub struct SvgNode<'a, 'input: 'a> {
    id: NodeId,
    doc: &'a Document<'input>,
    d: &'a NodeData,
}

impl Eq for SvgNode<'_, '_> {}

impl PartialEq for SvgNode<'_, '_> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && std::ptr::eq(self.doc, other.doc) && std::ptr::eq(self.d, other.d)
    }
}

impl<'a, 'input: 'a> SvgNode<'a, 'input> {
    #[inline]
    fn id(&self) -> NodeId {
        self.id
    }

    /// Checks if the current node is an element.
    #[inline]
    pub fn is_element(&self) -> bool {
        matches!(self.d.kind, NodeKind::Element { .. })
    }

    /// Checks if the current node is a text.
    #[inline]
    pub fn is_text(&self) -> bool {
        matches!(self.d.kind, NodeKind::Text(_))
    }

    /// Returns node's document.
    #[inline]
    pub fn document(&self) -> &'a Document<'input> {
        self.doc
    }

    /// Returns element's tag name, unless the current node is text.
    #[inline]
    pub fn tag_name(&self) -> Option<EId> {
        match self.d.kind {
            NodeKind::Element { tag_name, .. } => Some(tag_name),
            _ => None,
        }
    }
    /// Returns element's `id` attribute value.
    ///
    /// Returns an empty string otherwise.
    #[inline]
    pub fn element_id(&self) -> &'a str {
        self.attribute(AId::Id).unwrap_or("")
    }

    /// Returns an attribute value.
    pub fn attribute<T: FromValue<'a, 'input>>(&self, aid: AId) -> Option<T> {
        let attr = self.attributes().iter().find(|a| a.name == aid)?;

        T::parse(*self, aid, attr.value.as_ref())
    }

    pub fn attribute_value(&'a self, aid: AId) -> Option<SvgAttributeValueRef<'a>> {
        let attr = self.attributes().iter().find(|a| a.name == aid)?;
        Some(attr.value.as_ref())
    }

    /// Returns an attribute value.
    ///
    /// Same as `SvgNode::attribute`, but doesn't show a warning.
    pub fn try_attribute<T: FromValue<'a, 'input>>(&self, aid: AId) -> Option<T> {
        let attr = self.attributes().iter().find(|a| a.name == aid)?;
        T::parse(*self, aid, attr.value.as_ref())
    }

    #[inline]
    fn node_attribute(&self, aid: AId) -> Option<SvgNode<'a, 'input>> {
        let value = self.attribute(aid)?;
        let id = if aid == AId::Href {
            svgrtypes::IRI::from_str(value).ok().map(|v| v.0)
        } else {
            svgrtypes::FuncIRI::from_str(value).ok().map(|v| v.0)
        }?;

        self.document().element_by_id(id)
    }

    /// Checks if an attribute is present.
    #[inline]
    pub fn has_attribute(&self, aid: AId) -> bool {
        self.attributes().iter().any(|a| a.name == aid)
    }

    /// Returns a list of all element's attributes.
    #[inline]
    pub fn attributes(&self) -> &'a [Attribute<'input>] {
        match self.d.kind {
            NodeKind::Element { ref attributes, .. } => &self.doc.attrs[attributes.to_urange()],
            _ => &[],
        }
    }

    #[inline]
    fn attribute_id(&self, aid: AId) -> Option<usize> {
        match self.d.kind {
            NodeKind::Element { ref attributes, .. } => {
                let idx = self.attributes().iter().position(|attr| attr.name == aid)?;
                Some(attributes.start as usize + idx)
            }
            _ => None,
        }
    }

    /// Finds a [`Node`] that contains the required attribute.
    ///
    /// For inheritable attributes walks over ancestors until a node with
    /// the specified attribute is found.
    ///
    /// For non-inheritable attributes checks only the current node and the parent one.
    /// As per SVG spec.
    pub fn find_attribute<T: FromValue<'a, 'input>>(&self, aid: AId) -> Option<T> {
        self.find_attribute_impl(aid)?.attribute(aid)
    }

    fn find_attribute_impl(&self, aid: AId) -> Option<SvgNode<'a, 'input>> {
        if aid.is_inheritable() {
            for n in self.ancestors() {
                if n.has_attribute(aid) {
                    return Some(n);
                }
            }

            None
        } else {
            if self.has_attribute(aid) {
                Some(*self)
            } else {
                // Non-inheritable attributes can inherit a value only from a direct parent.
                let n = self.parent_element()?;
                if n.has_attribute(aid) {
                    Some(n)
                } else {
                    None
                }
            }
        }
    }

    /// Returns node's text data.
    ///
    /// For text nodes returns its content. For elements returns the first child node text.
    #[inline]
    pub fn text(&self) -> &'a str {
        match self.d.kind {
            NodeKind::Element { .. } => match self.first_child() {
                Some(child) if child.is_text() => match self.doc.nodes[child.id.get_usize()].kind {
                    NodeKind::Text(ref text) => text,
                    _ => "",
                },
                _ => "",
            },
            NodeKind::Text(ref text) => text,
            _ => "",
        }
    }

    /// Returns a parent node.
    #[inline]
    pub fn parent(&self) -> Option<Self> {
        self.d.parent.map(|id| self.doc.get(id))
    }

    /// Returns the parent element.
    #[inline]
    pub fn parent_element(&self) -> Option<Self> {
        self.ancestors().skip(1).find(|n| n.is_element())
    }

    /// Returns the next sibling.
    #[inline]
    pub fn next_sibling(&self) -> Option<Self> {
        self.d.next_sibling.map(|id| self.doc.get(id))
    }

    /// Returns the first child.
    #[inline]
    pub fn first_child(&self) -> Option<Self> {
        self.d.children.map(|(id, _)| self.doc.get(id))
    }

    /// Returns the first child element.
    #[inline]
    pub fn first_element_child(&self) -> Option<Self> {
        self.children().find(|n| n.is_element())
    }

    /// Returns the last child.
    #[inline]
    pub fn last_child(&self) -> Option<Self> {
        self.d.children.map(|(_, id)| self.doc.get(id))
    }

    /// Checks if the node has child nodes.
    #[inline]
    pub fn has_children(&self) -> bool {
        self.d.children.is_some()
    }

    /// Returns an iterator over ancestor nodes starting at this node.
    #[inline]
    pub fn ancestors(&self) -> Ancestors<'a, 'input> {
        Ancestors(Some(*self))
    }

    /// Returns an iterator over children nodes.
    #[inline]
    pub fn children(&self) -> Children<'a, 'input> {
        Children {
            front: self.first_child(),
            back: self.last_child(),
        }
    }

    /// Returns an iterator which traverses the subtree starting at this node.
    #[inline]
    fn traverse(&self) -> Traverse<'a, 'input> {
        Traverse {
            root: *self,
            edge: None,
        }
    }

    /// Returns an iterator over this node and its descendants.
    #[inline]
    pub fn descendants(&self) -> Descendants<'a, 'input> {
        Descendants(self.traverse())
    }

    /// Returns an iterator over elements linked via `xlink:href`.
    #[inline]
    pub fn href_iter(&self) -> HrefIter<'a, 'input> {
        HrefIter {
            doc: self.document(),
            origin: self.id(),
            curr: self.id(),
            is_first: true,
            is_finished: false,
        }
    }
}

impl std::fmt::Debug for SvgNode<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self.d.kind {
            NodeKind::Root => write!(f, "Root"),
            NodeKind::Element { .. } => {
                write!(
                    f,
                    "Element {{ tag_name: {:?}, attributes: {:?} }}",
                    self.tag_name(),
                    self.attributes()
                )
            }
            NodeKind::Text(ref text) => write!(f, "Text({:?})", text),
        }
    }
}

/// An iterator over ancestor nodes.
#[derive(Clone, Debug)]
pub struct Ancestors<'a, 'input: 'a>(Option<SvgNode<'a, 'input>>);

impl<'a, 'input: 'a> Iterator for Ancestors<'a, 'input> {
    type Item = SvgNode<'a, 'input>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let node = self.0.take();
        self.0 = node.as_ref().and_then(SvgNode::parent);
        node
    }
}

/// An iterator over children nodes.
#[derive(Clone, Debug)]
pub struct Children<'a, 'input: 'a> {
    front: Option<SvgNode<'a, 'input>>,
    back: Option<SvgNode<'a, 'input>>,
}

impl<'a, 'input: 'a> Iterator for Children<'a, 'input> {
    type Item = SvgNode<'a, 'input>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.front.take();
        if self.front == self.back {
            self.back = None;
        } else {
            self.front = node.as_ref().and_then(SvgNode::next_sibling);
        }
        node
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Edge<'a, 'input: 'a> {
    Open(SvgNode<'a, 'input>),
    Close(SvgNode<'a, 'input>),
}

#[derive(Clone, Debug)]
struct Traverse<'a, 'input: 'a> {
    root: SvgNode<'a, 'input>,
    edge: Option<Edge<'a, 'input>>,
}

impl<'a, 'input: 'a> Iterator for Traverse<'a, 'input> {
    type Item = Edge<'a, 'input>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.edge {
            Some(Edge::Open(node)) => {
                self.edge = Some(match node.first_child() {
                    Some(first_child) => Edge::Open(first_child),
                    None => Edge::Close(node),
                });
            }
            Some(Edge::Close(node)) => {
                if node == self.root {
                    self.edge = None;
                } else if let Some(next_sibling) = node.next_sibling() {
                    self.edge = Some(Edge::Open(next_sibling));
                } else {
                    self.edge = node.parent().map(Edge::Close);
                }
            }
            None => {
                self.edge = Some(Edge::Open(self.root));
            }
        }

        self.edge
    }
}

/// A descendants iterator.
#[derive(Clone, Debug)]
pub struct Descendants<'a, 'input: 'a>(Traverse<'a, 'input>);

impl<'a, 'input: 'a> Iterator for Descendants<'a, 'input> {
    type Item = SvgNode<'a, 'input>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        for edge in &mut self.0 {
            if let Edge::Open(node) = edge {
                return Some(node);
            }
        }

        None
    }
}

/// An iterator over `xlink:href` references.
#[derive(Clone, Debug)]
pub struct HrefIter<'a, 'input: 'a> {
    doc: &'a Document<'input>,
    origin: NodeId,
    curr: NodeId,
    is_first: bool,
    is_finished: bool,
}

impl<'a, 'input: 'a> Iterator for HrefIter<'a, 'input> {
    type Item = SvgNode<'a, 'input>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_finished {
            return None;
        }

        if self.is_first {
            self.is_first = false;
            return Some(self.doc.get(self.curr));
        }

        if let Some(link) = self.doc.get(self.curr).node_attribute(AId::Href) {
            if link.id() == self.curr || link.id() == self.origin {
                log::warn!(
                    "Element '#{}' cannot reference itself via 'xlink:href'.",
                    self.doc.get(self.origin).element_id()
                );
                self.is_finished = true;
                return None;
            }

            self.curr = link.id();
            Some(self.doc.get(self.curr))
        } else {
            None
        }
    }
}

impl EId {
    /// Checks if this is a
    /// [graphics element](https://www.w3.org/TR/SVG11/intro.html#TermGraphicsElement).
    pub fn is_graphic(&self) -> bool {
        matches!(
            self,
            EId::Circle
                | EId::Ellipse
                | EId::Image
                | EId::Line
                | EId::Path
                | EId::Polygon
                | EId::Polyline
                | EId::Rect
                | EId::Text
                | EId::Use
        )
    }

    /// Checks if this is a
    /// [gradient element](https://www.w3.org/TR/SVG11/intro.html#TermGradientElement).
    pub fn is_gradient(&self) -> bool {
        matches!(self, EId::LinearGradient | EId::RadialGradient)
    }

    /// Checks if this is a
    /// [paint server element](https://www.w3.org/TR/SVG11/intro.html#TermPaint).
    pub fn is_paint_server(&self) -> bool {
        matches!(
            self,
            EId::LinearGradient | EId::RadialGradient | EId::Pattern
        )
    }
}

impl AId {
    fn is_presentation(&self) -> bool {
        matches!(
            self,
            AId::AlignmentBaseline
                | AId::BaselineShift
                | AId::ClipPath
                | AId::ClipRule
                | AId::Color
                | AId::ColorInterpolation
                | AId::ColorInterpolationFilters
                | AId::ColorRendering
                | AId::Direction
                | AId::Display
                | AId::DominantBaseline
                | AId::Fill
                | AId::FillOpacity
                | AId::FillRule
                | AId::Filter
                | AId::FloodColor
                | AId::FloodOpacity
                | AId::FontFamily
                | AId::FontKerning // technically not presentation
                | AId::FontSize
                | AId::FontSizeAdjust
                | AId::FontStretch
                | AId::FontStyle
                | AId::FontVariant
                | AId::FontWeight
                | AId::GlyphOrientationHorizontal
                | AId::GlyphOrientationVertical
                | AId::ImageRendering
                | AId::Isolation // technically not presentation
                | AId::LetterSpacing
                | AId::LightingColor
                | AId::MarkerEnd
                | AId::MarkerMid
                | AId::MarkerStart
                | AId::Mask
                | AId::MaskType
                | AId::MixBlendMode // technically not presentation
                | AId::Opacity
                | AId::Overflow
                | AId::PaintOrder
                | AId::ShapeRendering
                | AId::StopColor
                | AId::StopOpacity
                | AId::Stroke
                | AId::StrokeDasharray
                | AId::StrokeDashoffset
                | AId::StrokeLinecap
                | AId::StrokeLinejoin
                | AId::StrokeMiterlimit
                | AId::StrokeOpacity
                | AId::StrokeWidth
                | AId::TextAnchor
                | AId::TextDecoration
                | AId::TextOverflow
                | AId::TextRendering
                | AId::Transform
                | AId::TransformOrigin
                | AId::UnicodeBidi
                | AId::VectorEffect
                | AId::Visibility
                | AId::WhiteSpace
                | AId::WordSpacing
                | AId::WritingMode
        )
    }

    /// Checks if the current attribute is inheritable.
    fn is_inheritable(&self) -> bool {
        if self.is_presentation() {
            !is_non_inheritable(*self)
        } else {
            false
        }
    }

    fn allows_inherit_value(&self) -> bool {
        matches!(
            self,
            AId::AlignmentBaseline
                | AId::BaselineShift
                | AId::ClipPath
                | AId::ClipRule
                | AId::Color
                | AId::ColorInterpolationFilters
                | AId::Direction
                | AId::Display
                | AId::DominantBaseline
                | AId::Fill
                | AId::FillOpacity
                | AId::FillRule
                | AId::Filter
                | AId::FloodColor
                | AId::FloodOpacity
                | AId::FontFamily
                | AId::FontKerning
                | AId::FontSize
                | AId::FontStretch
                | AId::FontStyle
                | AId::FontVariant
                | AId::FontWeight
                | AId::ImageRendering
                | AId::Kerning
                | AId::LetterSpacing
                | AId::MarkerEnd
                | AId::MarkerMid
                | AId::MarkerStart
                | AId::Mask
                | AId::Opacity
                | AId::Overflow
                | AId::ShapeRendering
                | AId::StopColor
                | AId::StopOpacity
                | AId::Stroke
                | AId::StrokeDasharray
                | AId::StrokeDashoffset
                | AId::StrokeLinecap
                | AId::StrokeLinejoin
                | AId::StrokeMiterlimit
                | AId::StrokeOpacity
                | AId::StrokeWidth
                | AId::TextAnchor
                | AId::TextDecoration
                | AId::TextRendering
                | AId::Visibility
                | AId::WordSpacing
                | AId::WritingMode
        )
    }
}

fn is_non_inheritable(id: AId) -> bool {
    matches!(
        id,
        AId::AlignmentBaseline
            | AId::BaselineShift
            | AId::ClipPath
            | AId::Display
            | AId::DominantBaseline
            | AId::Filter
            | AId::FloodColor
            | AId::FloodOpacity
            | AId::Mask
            | AId::Opacity
            | AId::Overflow
            | AId::LightingColor
            | AId::StopColor
            | AId::StopOpacity
            | AId::TextDecoration
            | AId::Transform
            | AId::TransformOrigin
    )
}

#[derive(Debug)]
pub enum SvgAttributeValueRef<'a> {
    Str(&'a str),
    // We store both string and float to avoid any potential conflict when the string
    // attribute is represented as a number, storing static string is cheaper than
    // converting floats all the time
    Float(f32, &'a str),
    Length(svgrtypes::Length),
    Transform(svgrtypes::Transform),
    Color(svgrtypes::Color),
    ImageData(&'a Arc<PreloadedImageData>),
}

impl<'a> SvgAttributeValueRef<'a> {
    pub fn as_str(&self) -> Option<&'a str> {
        match self {
            SvgAttributeValueRef::Str(s) => Some(s),
            SvgAttributeValueRef::Float(_, s) => Some(s),
            _ => None,
        }
    }
}

pub trait FromValue<'a, 'input: 'a>: Sized {
    fn parse(node: SvgNode<'a, 'input>, aid: AId, value: SvgAttributeValueRef<'a>) -> Option<Self>;
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for &'a str {
    fn parse(node: SvgNode<'a, 'input>, aid: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let str_value = value.as_str();

        if let None = str_value {
            log::error!("SVG rendering critical error: Attr `{aid:?}` requested a string type value on the node {node:?} but received a {value:?}. If you believe that the typing is correct file an issue at https://github.com/dmtrKovalenko/fframes ASAP.");
        }

        str_value
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for f32 {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        match value {
            SvgAttributeValueRef::Float(f, _) => Some(f),
            _ => svgrtypes::Number::from_str(value.as_str()?)
                .ok()
                .map(|v| v.0 as f32),
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::Length {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        match value {
            SvgAttributeValueRef::Length(l) => Some(l),
            SvgAttributeValueRef::Float(number, _) => Some(svgrtypes::Length {
                number: number as f64,
                unit: LengthUnit::None,
            }),
            _ => svgrtypes::Length::from_str(value.as_str()?).ok(),
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Opacity {
    fn parse(node: SvgNode, aid: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let length = svgrtypes::Length::parse(node, aid, value)?;

        if length.unit == svgrtypes::LengthUnit::Percent {
            Some(Opacity::new_clamped(length.number as f32 / 100.0))
        } else if length.unit == svgrtypes::LengthUnit::None {
            Some(Opacity::new_clamped(length.number as f32))
        } else {
            None
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Transform {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let ts = match value {
            SvgAttributeValueRef::Transform(t) => t,
            _ => {
                let s = value.as_str()?;
                svgrtypes::Transform::from_str(s).ok()?
            }
        };

        let ts = Transform::from_row(
            ts.a as f32,
            ts.b as f32,
            ts.c as f32,
            ts.d as f32,
            ts.e as f32,
            ts.f as f32,
        );

        if ts.is_valid() {
            Some(ts)
        } else {
            Some(Transform::default())
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MaybeTransform {
    Valid(Transform),
    Invalid,
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for MaybeTransform {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let ts = match value {
            SvgAttributeValueRef::Transform(t) => t,
            SvgAttributeValueRef::Str(text) => svgrtypes::Transform::from_str(text).ok()?,
            _ => return None,
        };

        let ts = Transform::from_row(
            ts.a as f32,
            ts.b as f32,
            ts.c as f32,
            ts.d as f32,
            ts.e as f32,
            ts.f as f32,
        );

        if ts.is_valid() {
            Some(MaybeTransform::Valid(ts))
        } else {
            Some(MaybeTransform::Invalid)
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::TransformOrigin {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::ViewBox {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Units {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        match value.as_str()? {
            "userSpaceOnUse" => Some(Units::UserSpaceOnUse),
            "objectBoundingBox" => Some(Units::ObjectBoundingBox),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::AspectRatio {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::PaintOrder {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::Color {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        match value {
            SvgAttributeValueRef::Str(str) => Self::from_str(str).ok(),
            SvgAttributeValueRef::Color(color) => Some(color),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::Angle {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::EnableBackground {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for svgrtypes::Paint<'a> {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        Self::from_str(value.as_str()?).ok()
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Vec<f32> {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        let mut list = Vec::new();
        for n in svgrtypes::NumberListParser::from(s) {
            list.push(n.ok()? as f32);
        }

        Some(list)
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Vec<svgrtypes::Length> {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        let mut list = Vec::new();
        for n in svgrtypes::LengthListParser::from(s) {
            list.push(n.ok()?);
        }

        Some(list)
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for Visibility {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "visible" => Some(Visibility::Visible),
            "hidden" => Some(Visibility::Hidden),
            "collapse" => Some(Visibility::Collapse),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for SpreadMethod {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "pad" => Some(SpreadMethod::Pad),
            "reflect" => Some(SpreadMethod::Reflect),
            "repeat" => Some(SpreadMethod::Repeat),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for ShapeRendering {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "optimizeSpeed" => Some(ShapeRendering::OptimizeSpeed),
            "crispEdges" => Some(ShapeRendering::CrispEdges),
            "auto" | "geometricPrecision" => Some(ShapeRendering::GeometricPrecision),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for TextRendering {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "optimizeSpeed" => Some(TextRendering::OptimizeSpeed),
            "auto" | "optimizeLegibility" => Some(TextRendering::OptimizeLegibility),
            "geometricPrecision" => Some(TextRendering::GeometricPrecision),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for ImageRendering {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "auto" | "optimizeQuality" => Some(ImageRendering::OptimizeQuality),
            "optimizeSpeed" => Some(ImageRendering::OptimizeSpeed),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for BlendMode {
    fn parse(_: SvgNode, _: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        match s {
            "normal" => Some(BlendMode::Normal),
            "multiply" => Some(BlendMode::Multiply),
            "screen" => Some(BlendMode::Screen),
            "overlay" => Some(BlendMode::Overlay),
            "darken" => Some(BlendMode::Darken),
            "lighten" => Some(BlendMode::Lighten),
            "color-dodge" => Some(BlendMode::ColorDodge),
            "color-burn" => Some(BlendMode::ColorBurn),
            "hard-light" => Some(BlendMode::HardLight),
            "soft-light" => Some(BlendMode::SoftLight),
            "difference" => Some(BlendMode::Difference),
            "exclusion" => Some(BlendMode::Exclusion),
            "hue" => Some(BlendMode::Hue),
            "saturation" => Some(BlendMode::Saturation),
            "color" => Some(BlendMode::Color),
            "luminosity" => Some(BlendMode::Luminosity),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for SvgNode<'a, 'input> {
    fn parse(node: SvgNode<'a, 'input>, aid: AId, value: SvgAttributeValueRef<'a>) -> Option<Self> {
        let s = value.as_str()?;
        let id = if aid == AId::Href {
            svgrtypes::IRI::from_str(s).ok().map(|v| v.0)?
        } else {
            svgrtypes::FuncIRI::from_str(s).ok().map(|v| v.0)?
        };
        node.document().element_by_id(id)
    }
}
