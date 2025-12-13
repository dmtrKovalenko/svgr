// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub mod filter;
mod geom;
mod text;

use std::fmt::Display;
use std::sync::Arc;
pub use strict_num::{self, ApproxEqUlps, NonZeroPositiveF32, NormalizedF32, PositiveF32};
pub use svgrtypes::{Align, AspectRatio};

pub use tiny_skia_path;

use crate::hashers::CustomHash;
use crate::PreloadedImageData;

pub use self::geom::*;
pub use self::text::*;

/// An alias to `NormalizedF32`.
pub type Opacity = NormalizedF32;

// Must not be clone-able to preserve ID uniqueness.
#[derive(Debug, Hash)]
pub(crate) struct NonEmptyString(String);

impl NonEmptyString {
    pub(crate) fn new(string: String) -> Option<Self> {
        if string.trim().is_empty() {
            return None;
        }

        Some(NonEmptyString(string))
    }

    pub(crate) fn get(&self) -> &str {
        &self.0
    }

    pub(crate) fn take(self) -> String {
        self.0
    }
}

/// A non-zero `f32`.
///
/// Just like `f32` but immutable and guarantee to never be zero.
#[derive(Clone, Copy, Debug)]
pub struct NonZeroF32(f32);

impl std::hash::Hash for NonZeroF32 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl NonZeroF32 {
    /// Creates a new `NonZeroF32` value.
    #[inline]
    pub fn new(n: f32) -> Option<Self> {
        if n.approx_eq_ulps(&0.0, 4) {
            None
        } else {
            Some(NonZeroF32(n))
        }
    }

    /// Returns an underlying value.
    #[inline]
    pub fn get(&self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub(crate) enum Units {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

// `Units` cannot have a default value, because it changes depending on an element.

/// A visibility property.
///
/// `visibility` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
    Collapse,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Visible
    }
}

/// A shape rendering method.
///
/// `shape-rendering` attribute in the SVG.
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
#[allow(missing_docs)]
pub enum ShapeRendering {
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

impl ShapeRendering {
    /// Checks if anti-aliasing should be enabled.
    pub fn use_shape_antialiasing(self) -> bool {
        match self {
            ShapeRendering::OptimizeSpeed => false,
            ShapeRendering::CrispEdges => false,
            ShapeRendering::GeometricPrecision => true,
        }
    }
}

impl Default for ShapeRendering {
    fn default() -> Self {
        Self::GeometricPrecision
    }
}

impl std::str::FromStr for ShapeRendering {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "optimizeSpeed" => Ok(ShapeRendering::OptimizeSpeed),
            "crispEdges" => Ok(ShapeRendering::CrispEdges),
            "geometricPrecision" => Ok(ShapeRendering::GeometricPrecision),
            _ => Err("invalid"),
        }
    }
}

/// A text rendering method.
///
/// `text-rendering` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum TextRendering {
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

impl Default for TextRendering {
    fn default() -> Self {
        Self::OptimizeLegibility
    }
}

impl std::str::FromStr for TextRendering {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "optimizeSpeed" => Ok(TextRendering::OptimizeSpeed),
            "optimizeLegibility" => Ok(TextRendering::OptimizeLegibility),
            "geometricPrecision" => Ok(TextRendering::GeometricPrecision),
            _ => Err("invalid"),
        }
    }
}

/// An image rendering method.
///
/// `image-rendering` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum ImageRendering {
    OptimizeQuality,
    OptimizeSpeed,
}

impl Default for ImageRendering {
    fn default() -> Self {
        Self::OptimizeQuality
    }
}

impl std::str::FromStr for ImageRendering {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "optimizeQuality" => Ok(ImageRendering::OptimizeQuality),
            "optimizeSpeed" => Ok(ImageRendering::OptimizeSpeed),
            "smooth" | "high-quality" => Ok(ImageRendering::OptimizeQuality),
            "crisp-edges" | "pixelated" => Ok(ImageRendering::OptimizeSpeed),
            _ => Err("invalid"),
        }
    }
}

/// A blending mode property.
///
/// `mix-blend-mode` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

impl Display for BlendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let blend_mode = match self {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
            BlendMode::Screen => "screen",
            BlendMode::Overlay => "overlay",
            BlendMode::Darken => "darken",
            BlendMode::Lighten => "lighten",
            BlendMode::ColorDodge => "color-dodge",
            BlendMode::ColorBurn => "color-burn",
            BlendMode::HardLight => "hard-light",
            BlendMode::SoftLight => "soft-light",
            BlendMode::Difference => "difference",
            BlendMode::Exclusion => "exclusion",
            BlendMode::Hue => "hue",
            BlendMode::Saturation => "saturation",
            BlendMode::Color => "color",
            BlendMode::Luminosity => "luminosity",
        };
        write!(f, "{blend_mode}")
    }
}

/// A spread method.
///
/// `spreadMethod` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum SpreadMethod {
    Pad,
    Reflect,
    Repeat,
}

impl Default for SpreadMethod {
    fn default() -> Self {
        Self::Pad
    }
}

/// A generic gradient.
#[derive(Debug)]
pub struct BaseGradient {
    pub(crate) id: NonEmptyString,
    pub(crate) units: Units, // used only during parsing
    pub(crate) transform: Transform,
    pub(crate) spread_method: SpreadMethod,
    pub(crate) stops: Vec<Stop>,
}

impl std::hash::Hash for BaseGradient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.transform.custom_hash(state);
        self.spread_method.hash(state);
        self.stops.hash(state);
    }
}

impl BaseGradient {
    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Used only during SVG writing. `svgr` doesn't rely on this property.
    pub fn id(&self) -> &str {
        self.id.get()
    }

    /// Gradient transform.
    ///
    /// `gradientTransform` in SVG.
    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Gradient spreading method.
    ///
    /// `spreadMethod` in SVG.
    pub fn spread_method(&self) -> SpreadMethod {
        self.spread_method
    }

    /// A list of `stop` elements.
    pub fn stops(&self) -> &[Stop] {
        &self.stops
    }
}

/// A linear gradient.
///
/// `linearGradient` element in SVG.
#[derive(Debug)]
pub struct LinearGradient {
    pub(crate) base: BaseGradient,
    pub(crate) x1: f32,
    pub(crate) y1: f32,
    pub(crate) x2: f32,
    pub(crate) y2: f32,
}

impl std::hash::Hash for LinearGradient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.x1.to_bits().hash(state);
        self.y1.to_bits().hash(state);
        self.x2.to_bits().hash(state);
        self.y2.to_bits().hash(state);
        self.base.hash(state);
    }
}

impl LinearGradient {
    /// `x1` coordinate.
    pub fn x1(&self) -> f32 {
        self.x1
    }

    /// `y1` coordinate.
    pub fn y1(&self) -> f32 {
        self.y1
    }

    /// `x2` coordinate.
    pub fn x2(&self) -> f32 {
        self.x2
    }

    /// `y2` coordinate.
    pub fn y2(&self) -> f32 {
        self.y2
    }
}

impl std::ops::Deref for LinearGradient {
    type Target = BaseGradient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

/// A radial gradient.
///
/// `radialGradient` element in SVG.
#[derive(Debug)]
pub struct RadialGradient {
    pub(crate) base: BaseGradient,
    pub(crate) cx: f32,
    pub(crate) cy: f32,
    pub(crate) r: PositiveF32,
    pub(crate) fx: f32,
    pub(crate) fy: f32,
}

impl std::hash::Hash for RadialGradient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.cx.to_bits().hash(state);
        self.cy.to_bits().hash(state);
        self.r.hash(state);
        self.fx.to_bits().hash(state);
        self.fy.to_bits().hash(state);
        self.base.hash(state);
    }
}

impl RadialGradient {
    /// `cx` coordinate.
    pub fn cx(&self) -> f32 {
        self.cx
    }

    /// `cy` coordinate.
    pub fn cy(&self) -> f32 {
        self.cy
    }

    /// Gradient radius.
    pub fn r(&self) -> PositiveF32 {
        self.r
    }

    /// `fx` coordinate.
    pub fn fx(&self) -> f32 {
        self.fx
    }

    /// `fy` coordinate.
    pub fn fy(&self) -> f32 {
        self.fy
    }
}

impl std::ops::Deref for RadialGradient {
    type Target = BaseGradient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

/// An alias to `NormalizedF32`.
pub type StopOffset = NormalizedF32;

/// Gradient's stop element.
///
/// `stop` element in SVG.
#[derive(Clone, Copy, Debug, Hash)]
pub struct Stop {
    pub(crate) offset: StopOffset,
    pub(crate) color: Color,
    pub(crate) opacity: Opacity,
}

impl Stop {
    /// Gradient stop offset.
    ///
    /// `offset` in SVG.
    pub fn offset(&self) -> StopOffset {
        self.offset
    }

    /// Gradient stop color.
    ///
    /// `stop-color` in SVG.
    pub fn color(&self) -> Color {
        self.color
    }

    /// Gradient stop opacity.
    ///
    /// `stop-opacity` in SVG.
    pub fn opacity(&self) -> Opacity {
        self.opacity
    }
}

/// A pattern element.
///
/// `pattern` element in SVG.
#[derive(Debug)]
pub struct Pattern {
    pub(crate) id: NonEmptyString,
    pub(crate) units: Units,         // used only during parsing
    pub(crate) content_units: Units, // used only during parsing
    pub(crate) transform: Transform,
    pub(crate) rect: NonZeroRect,
    pub(crate) view_box: Option<ViewBox>,
    pub(crate) root: Group,
}

impl std::hash::Hash for Pattern {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.transform.custom_hash(state);
        self.rect.custom_hash(state);
        self.view_box.hash(state);
        self.root.hash(state);
    }
}

impl Pattern {
    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Used only during SVG writing. `svgr` doesn't rely on this property.
    pub fn id(&self) -> &str {
        self.id.get()
    }

    /// Pattern transform.
    ///
    /// `patternTransform` in SVG.
    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Pattern rectangle.
    ///
    /// `x`, `y`, `width` and `height` in SVG.
    pub fn rect(&self) -> NonZeroRect {
        self.rect
    }

    /// Pattern viewbox.
    pub fn view_box(&self) -> Option<ViewBox> {
        self.view_box
    }

    /// Pattern children.
    pub fn root(&self) -> &Group {
        &self.root
    }
}

/// An alias to `NonZeroPositiveF32`.
pub type StrokeWidth = NonZeroPositiveF32;

/// A `stroke-miterlimit` value.
///
/// Just like `f32` but immutable and guarantee to be >=1.0.
#[derive(Clone, Copy, Debug)]
pub struct StrokeMiterlimit(f32);

impl StrokeMiterlimit {
    /// Creates a new `StrokeMiterlimit` value.
    #[inline]
    pub fn new(n: f32) -> Self {
        debug_assert!(n.is_finite());
        debug_assert!(n >= 1.0);

        let n = if !(n >= 1.0) { 1.0 } else { n };

        StrokeMiterlimit(n)
    }

    /// Returns an underlying value.
    #[inline]
    pub fn get(&self) -> f32 {
        self.0
    }
}

impl Default for StrokeMiterlimit {
    #[inline]
    fn default() -> Self {
        StrokeMiterlimit::new(4.0)
    }
}

impl From<f32> for StrokeMiterlimit {
    #[inline]
    fn from(n: f32) -> Self {
        Self::new(n)
    }
}

impl PartialEq for StrokeMiterlimit {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.approx_eq_ulps(&other.0, 4)
    }
}

/// A line cap.
///
/// `stroke-linecap` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

impl Default for LineCap {
    fn default() -> Self {
        Self::Butt
    }
}

/// A line join.
///
/// `stroke-linejoin` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum LineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

impl Default for LineJoin {
    fn default() -> Self {
        Self::Miter
    }
}

/// A stroke style.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub(crate) paint: Paint,
    pub(crate) dasharray: Option<Vec<f32>>,
    pub(crate) dashoffset: f32,
    pub(crate) miterlimit: StrokeMiterlimit,
    pub(crate) opacity: Opacity,
    pub(crate) width: StrokeWidth,
    pub(crate) linecap: LineCap,
    pub(crate) linejoin: LineJoin,
    // Whether the current stroke needs to be resolved relative
    // to a context element.
    pub(crate) context_element: Option<ContextElement>,
}

impl std::hash::Hash for Stroke {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.paint.hash(state);
        self.dasharray
            .as_ref()
            .map(|vec| vec.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
            .hash(state);
        self.dashoffset.to_bits().hash(state);

        self.miterlimit.0.to_bits().hash(state);
        self.opacity.hash(state);
        self.width.hash(state);
        self.linecap.hash(state);
        self.linejoin.hash(state);
    }
}

impl Stroke {
    /// Stroke paint.
    pub fn paint(&self) -> &Paint {
        &self.paint
    }

    /// Stroke dash array.
    pub fn dasharray(&self) -> Option<&[f32]> {
        self.dasharray.as_deref()
    }

    /// Stroke dash offset.
    pub fn dashoffset(&self) -> f32 {
        self.dashoffset
    }

    /// Stroke miter limit.
    pub fn miterlimit(&self) -> StrokeMiterlimit {
        self.miterlimit
    }

    /// Stroke opacity.
    pub fn opacity(&self) -> Opacity {
        self.opacity
    }

    /// Stroke width.
    pub fn width(&self) -> StrokeWidth {
        self.width
    }

    /// Stroke linecap.
    pub fn linecap(&self) -> LineCap {
        self.linecap
    }

    /// Stroke linejoin.
    pub fn linejoin(&self) -> LineJoin {
        self.linejoin
    }

    /// Converts into a `tiny_skia_path::Stroke` type.
    pub fn to_tiny_skia(&self) -> tiny_skia_path::Stroke {
        let mut stroke = tiny_skia_path::Stroke {
            width: self.width.get(),
            miter_limit: self.miterlimit.get(),
            line_cap: match self.linecap {
                LineCap::Butt => tiny_skia_path::LineCap::Butt,
                LineCap::Round => tiny_skia_path::LineCap::Round,
                LineCap::Square => tiny_skia_path::LineCap::Square,
            },
            line_join: match self.linejoin {
                LineJoin::Miter => tiny_skia_path::LineJoin::Miter,
                LineJoin::MiterClip => tiny_skia_path::LineJoin::MiterClip,
                LineJoin::Round => tiny_skia_path::LineJoin::Round,
                LineJoin::Bevel => tiny_skia_path::LineJoin::Bevel,
            },
            // According to the spec, dash should not be accounted during
            // bbox calculation.
            dash: None,
        };

        if let Some(ref list) = self.dasharray {
            stroke.dash = tiny_skia_path::StrokeDash::new(list.clone(), self.dashoffset);
        }

        stroke
    }
}

/// A fill rule.
///
/// `fill-rule` attribute in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

impl Default for FillRule {
    fn default() -> Self {
        Self::NonZero
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ContextElement {
    /// The current context element is a use node. Since we can get
    /// the bounding box of a use node only once we have converted
    /// all elements, we need to fix the transform and units of
    /// the stroke/fill after converting the whole tree.
    UseNode,
    /// The current context element is a path node (i.e. only applicable
    /// if we draw the marker of a path). Since we already know the bounding
    /// box of the path when rendering the markers, we can convert them directly,
    /// so we do it while parsing.
    PathNode(Transform, Option<NonZeroRect>),
}

impl std::hash::Hash for ContextElement {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            ContextElement::UseNode => 0.hash(state),
            ContextElement::PathNode(transform, rect) => {
                1.hash(state);
                transform.custom_hash(state);

                if let Some(rect) = rect {
                    rect.custom_hash(state);
                }
            }
        }
    }
}

/// A fill style.
#[derive(Clone, Debug, Hash)]
pub struct Fill {
    pub(crate) paint: Paint,
    pub(crate) opacity: Opacity,
    pub(crate) rule: FillRule,
    // Whether the current fill needs to be resolved relative
    // to a context element.
    pub(crate) context_element: Option<ContextElement>,
}

impl Fill {
    /// Fill paint.
    pub fn paint(&self) -> &Paint {
        &self.paint
    }

    /// Fill opacity.
    pub fn opacity(&self) -> Opacity {
        self.opacity
    }

    /// Fill rule.
    pub fn rule(&self) -> FillRule {
        self.rule
    }
}

impl Default for Fill {
    fn default() -> Self {
        Fill {
            paint: Paint::Color(Color::black()),
            opacity: Opacity::ONE,
            rule: FillRule::default(),
            context_element: None,
        }
    }
}

/// A 8-bit RGB color.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug, Hash)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    /// Constructs a new `Color` from RGB values.
    #[inline]
    pub fn new_rgb(red: u8, green: u8, blue: u8) -> Color {
        Color { red, green, blue }
    }

    /// Constructs a new `Color` set to black.
    #[inline]
    pub fn black() -> Color {
        Color::new_rgb(0, 0, 0)
    }

    /// Constructs a new `Color` set to white.
    #[inline]
    pub fn white() -> Color {
        Color::new_rgb(255, 255, 255)
    }
}

/// A paint style.
///
/// `paint` value type in the SVG.
#[allow(missing_docs)]
#[derive(Clone, Debug)]
pub enum Paint {
    Color(Color),
    LinearGradient(Arc<LinearGradient>),
    RadialGradient(Arc<RadialGradient>),
    Pattern(Arc<Pattern>),
}

impl PartialEq for Paint {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Color(lc), Self::Color(rc)) => lc == rc,
            (Self::LinearGradient(ref lg1), Self::LinearGradient(ref lg2)) => Arc::ptr_eq(lg1, lg2),
            (Self::RadialGradient(ref rg1), Self::RadialGradient(ref rg2)) => Arc::ptr_eq(rg1, rg2),
            (Self::Pattern(ref p1), Self::Pattern(ref p2)) => Arc::ptr_eq(p1, p2),
            _ => false,
        }
    }
}

impl std::hash::Hash for Paint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Paint::Color(color) => color.hash(state),
            Paint::LinearGradient(gradient) => gradient.id.hash(state),
            Paint::RadialGradient(gradient) => gradient.id.hash(state),
            Paint::Pattern(pattern) => pattern.id.hash(state),
        }
    }
}

/// A clip-path element.
///
/// `clipPath` element in SVG.
#[derive(Debug)]
pub struct ClipPath {
    pub(crate) id: NonEmptyString,
    pub(crate) transform: Transform,
    pub(crate) clip_path: Option<Arc<ClipPath>>,
    pub(crate) root: Group,
}

impl std::hash::Hash for ClipPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.transform.custom_hash(state);
        self.clip_path.hash(state);
        self.root.hash(state);
    }
}

impl ClipPath {
    pub(crate) fn empty(id: NonEmptyString) -> Self {
        ClipPath {
            id,
            transform: Transform::default(),
            clip_path: None,
            root: Group::empty(),
        }
    }

    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Used only during SVG writing. `svgr` doesn't rely on this property.
    pub fn id(&self) -> &str {
        self.id.get()
    }

    /// Clip path transform.
    ///
    /// `transform` in SVG.
    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Additional clip path.
    ///
    /// `clip-path` in SVG.
    pub fn clip_path(&self) -> Option<&ClipPath> {
        self.clip_path.as_deref()
    }

    /// Clip path children.
    pub fn root(&self) -> &Group {
        &self.root
    }
}

/// A mask type.
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
pub enum MaskType {
    /// Indicates that the luminance values of the mask should be used.
    Luminance,
    /// Indicates that the alpha values of the mask should be used.
    Alpha,
}

impl Default for MaskType {
    fn default() -> Self {
        Self::Luminance
    }
}

/// A mask element.
///
/// `mask` element in SVG.
#[derive(Debug)]
pub struct Mask {
    pub(crate) id: NonEmptyString,
    pub(crate) rect: NonZeroRect,
    pub(crate) kind: MaskType,
    pub(crate) mask: Option<Arc<Mask>>,
    pub(crate) root: Group,
}

impl std::hash::Hash for Mask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.rect.custom_hash(state);
        self.kind.hash(state);
        self.mask.hash(state);
        self.root.hash(state);
    }
}

impl Mask {
    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Used only during SVG writing. `svgr` doesn't rely on this property.
    pub fn id(&self) -> &str {
        self.id.get()
    }

    /// Mask rectangle.
    ///
    /// `x`, `y`, `width` and `height` in SVG.
    pub fn rect(&self) -> NonZeroRect {
        self.rect
    }

    /// Mask type.
    ///
    /// `mask-type` in SVG.
    pub fn kind(&self) -> MaskType {
        self.kind
    }

    /// Additional mask.
    ///
    /// `mask` in SVG.
    pub fn mask(&self) -> Option<&Mask> {
        self.mask.as_deref()
    }

    /// Mask children.
    ///
    /// A mask can have no children, in which case the whole element should be masked out.
    pub fn root(&self) -> &Group {
        &self.root
    }
}

/// Node's kind.
#[allow(missing_docs)]
#[derive(Clone, Debug, Hash)]
pub enum Node {
    Group(Box<Group>),
    Path(Box<Path>),
    Image(Box<Image>),
    Text(Box<Text>),
    FastShape(Box<FastShape>),
}

impl Node {
    /// Returns node's ID.
    pub fn id(&self) -> &str {
        match self {
            Node::Group(ref e) => e.id.as_str(),
            Node::Path(ref e) => e.id.as_str(),
            Node::Image(ref e) => e.id.as_str(),
            Node::Text(ref e) => e.id.as_str(),
            Node::FastShape(ref e) => e.path.id.as_str(),
        }
    }

    /// Returns node's absolute transform.
    ///
    /// If a current node doesn't support transformation - a default
    /// transform will be returned.
    ///
    /// This method is cheap since absolute transforms are already resolved.
    pub fn abs_transform(&self) -> Transform {
        match self {
            Node::Group(ref group) => group.abs_transform(),
            Node::Path(ref path) => path.abs_transform(),
            Node::Image(ref image) => image.abs_transform(),
            Node::Text(ref text) => text.abs_transform(),
            Node::FastShape(ref shape) => shape.path.abs_transform(),
        }
    }

    /// Returns node's bounding box in object coordinates, if any.
    pub fn bounding_box(&self) -> Rect {
        match self {
            Node::Group(ref group) => group.bounding_box(),
            Node::Path(ref path) => path.bounding_box(),
            Node::Image(ref image) => image.bounding_box(),
            Node::Text(ref text) => text.bounding_box(),
            Node::FastShape(ref shape) => shape.path.bounding_box(),
        }
    }

    /// Returns node's bounding box in canvas coordinates, if any.
    pub fn abs_bounding_box(&self) -> Rect {
        match self {
            Node::Group(ref group) => group.abs_bounding_box(),
            Node::Path(ref path) => path.abs_bounding_box(),
            Node::Image(ref image) => image.abs_bounding_box(),
            Node::Text(ref text) => text.abs_bounding_box(),
            Node::FastShape(ref shape) => shape.path.abs_bounding_box(),
        }
    }

    /// Returns node's bounding box, including stroke, in object coordinates, if any.
    pub fn stroke_bounding_box(&self) -> Rect {
        match self {
            Node::Group(ref group) => group.stroke_bounding_box(),
            Node::Path(ref path) => path.stroke_bounding_box(),
            // Image cannot be stroked.
            Node::Image(ref image) => image.bounding_box(),
            Node::Text(ref text) => text.stroke_bounding_box(),
            Node::FastShape(ref shape) => shape.path.stroke_bounding_box(),
        }
    }

    /// Returns node's bounding box, including stroke, in canvas coordinates, if any.
    pub fn abs_stroke_bounding_box(&self) -> Rect {
        match self {
            Node::Group(ref group) => group.abs_stroke_bounding_box(),
            Node::Path(ref path) => path.abs_stroke_bounding_box(),
            // Image cannot be stroked.
            Node::Image(ref image) => image.abs_bounding_box(),
            Node::Text(ref text) => text.abs_stroke_bounding_box(),
            Node::FastShape(ref shape) => shape.path.abs_stroke_bounding_box(),
        }
    }

    /// Element's "layer" bounding box in canvas units, if any.
    ///
    /// For most nodes this is just `abs_bounding_box`,
    /// but for groups this is `abs_layer_bounding_box`.
    ///
    /// See [`Group::layer_bounding_box`] for details.
    pub fn abs_layer_bounding_box(&self) -> Option<NonZeroRect> {
        match self {
            Node::Group(ref group) => Some(group.abs_layer_bounding_box()),
            // Hor/ver path without stroke can return None. This is expected.
            Node::Path(ref path) => path.abs_bounding_box().to_non_zero_rect(),
            Node::Image(ref image) => image.abs_bounding_box().to_non_zero_rect(),
            Node::Text(ref text) => text.abs_bounding_box().to_non_zero_rect(),
            Node::FastShape(ref shape) => shape.path.abs_bounding_box().to_non_zero_rect(),
        }
    }

    /// Calls a closure for each subroot this `Node` has.
    ///
    /// The [`Tree::root`](Tree::root) field contain only render-able SVG elements.
    /// But some elements, specifically clip paths, masks, patterns and feImage
    /// can store their own SVG subtrees.
    /// And while one can access them manually, it's pretty verbose.
    /// This methods allows looping over _all_ SVG elements present in the `Tree`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// fn all_nodes(parent: &usvgr::Group) {
    ///     for node in parent.children() {
    ///         // do stuff...
    ///
    ///         if let usvgr::Node::Group(ref g) = node {
    ///             all_nodes(g);
    ///         }
    ///
    ///         // handle subroots as well
    ///         node.subroots(|subroot| all_nodes(subroot));
    ///     }
    /// }
    /// ```
    pub fn subroots<F: FnMut(&Group)>(&self, mut f: F) {
        match self {
            Node::Group(ref group) => group.subroots(&mut f),
            Node::Path(ref path) => path.subroots(&mut f),
            Node::Image(ref image) => image.subroots(&mut f),
            Node::Text(ref text) => text.subroots(&mut f),
            Node::FastShape(ref shape) => shape.path.subroots(&mut f),
        }
    }
}

/// A group container.
///
/// The preprocessor will remove all groups that don't impact rendering.
/// Those that left is just an indicator that a new canvas should be created.
///
/// `g` element in SVG.
#[derive(Clone, Debug)]
pub struct Group {
    pub(crate) id: String,
    pub(crate) transform: Transform,
    pub(crate) abs_transform: Transform,
    pub(crate) opacity: Opacity,
    pub(crate) blend_mode: BlendMode,
    pub(crate) isolate: bool,
    pub(crate) clip_path: Option<Arc<ClipPath>>,
    /// Whether the group is a context element (i.e. a use node)
    pub(crate) is_context_element: bool,
    pub(crate) mask: Option<Arc<Mask>>,
    pub(crate) filters: Vec<Arc<filter::Filter>>,
    pub(crate) bounding_box: Rect,
    pub(crate) abs_bounding_box: Rect,
    pub(crate) stroke_bounding_box: Rect,
    pub(crate) abs_stroke_bounding_box: Rect,
    pub(crate) layer_bounding_box: NonZeroRect,
    pub(crate) abs_layer_bounding_box: NonZeroRect,
    pub(crate) children: Vec<Node>,
    /// Pre-computed hash for fully static groups (no runtime-varying content).
    /// If Some, the static cache can use this directly instead of computing at runtime.
    pub(crate) static_hash: Option<u64>,
}

impl std::hash::Hash for Group {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.transform.custom_hash(state);
        self.abs_transform.custom_hash(state);
        // do not hash opacity and blend_mode it is used after caching
        // self.opacity.hash(state);
        // self.blend_mode.hash(state);
        self.isolate.hash(state);
        self.clip_path.hash(state);
        self.mask.hash(state);
        self.filters.hash(state);
        self.bounding_box.custom_hash(state);
        self.abs_bounding_box.custom_hash(state);
        self.stroke_bounding_box.custom_hash(state);
        self.abs_stroke_bounding_box.custom_hash(state);
        self.layer_bounding_box.custom_hash(state);
        self.abs_layer_bounding_box.custom_hash(state);
        self.children.hash(state);
    }
}

impl Group {
    /// Hashes what the group draws in its own coordinate system: the group's own
    /// transform, opacity and blend mode and every absolute (canvas space) value are left
    /// out, so the same content placed elsewhere hashes the same. Renderers use it to reuse rendered layers of
    /// moving content.
    ///
    /// Returns `None` without a meaningful hash for content that can not be hashed that
    /// way (images, clip paths and masks, which hold absolute values).
    pub fn content_hash<H: std::hash::Hasher>(&self, state: &mut H) -> Option<()> {
        self.content_hash_impl(state, false)
    }

    fn content_hash_impl<H: std::hash::Hasher>(
        &self,
        state: &mut H,
        include_transform: bool,
    ) -> Option<()> {
        use crate::hashers::CustomHash;
        use std::hash::Hash;

        if self.clip_path.is_some() || self.mask.is_some() {
            return None;
        }

        // the outermost group's opacity and blend mode apply when its layer is drawn
        if include_transform {
            self.transform.custom_hash(state);
            self.opacity.hash(state);
            (self.blend_mode as u8).hash(state);
        }
        self.isolate.hash(state);
        self.filters.hash(state);

        self.children.len().hash(state);
        for child in &self.children {
            match child {
                Node::Group(group) => {
                    0u8.hash(state);
                    group.content_hash_impl(state, true)?;
                }
                Node::Text(text) => {
                    1u8.hash(state);
                    text.flattened().content_hash_impl(state, true)?;
                }
                Node::Path(_) | Node::FastShape(_) => {
                    let path = match child {
                        // The data is only the bounding rectangle, so the kind tells shapes apart.
                        Node::FastShape(shape) => {
                            shape.kind.hash(state);
                            &shape.path
                        }
                        Node::Path(path) => path,
                        _ => unreachable!(),
                    };
                    2u8.hash(state);
                    path.visibility.hash(state);
                    path.fill.hash(state);
                    path.stroke.hash(state);
                    path.paint_order.hash(state);
                    path.rendering_mode.hash(state);
                    path.data.custom_hash(state);
                }
                Node::Image(_) => return None,
            }
        }

        Some(())
    }

    pub(crate) fn empty() -> Self {
        let dummy = Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap();
        Group {
            id: String::new(),
            transform: Transform::default(),
            abs_transform: Transform::default(),
            opacity: Opacity::ONE,
            blend_mode: BlendMode::Normal,
            isolate: false,
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
            static_hash: None,
        }
    }

    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Isn't automatically generated.
    /// Can be empty.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Element's transform.
    ///
    /// This is a relative transform. The one that is set via the `transform` attribute in SVG.
    pub fn transform(&self) -> Transform {
        self.transform
    }

    /// Element's absolute transform.
    ///
    /// Contains all ancestors transforms including group's transform.
    ///
    /// Note that subroots, like clipPaths, masks and patterns, have their own root transform,
    /// which isn't affected by the node that references this subroot.
    pub fn abs_transform(&self) -> Transform {
        self.abs_transform
    }

    /// Group opacity.
    ///
    /// After the group is rendered we should combine
    /// it with a parent group using the specified opacity.
    pub fn opacity(&self) -> Opacity {
        self.opacity
    }

    /// Group blend mode.
    ///
    /// `mix-blend-mode` in SVG.
    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }

    /// Group isolation.
    ///
    /// `isolation` in SVG.
    pub fn isolate(&self) -> bool {
        self.isolate
    }

    /// Element's clip path.
    pub fn clip_path(&self) -> Option<&ClipPath> {
        self.clip_path.as_deref()
    }

    /// Element's mask.
    pub fn mask(&self) -> Option<&Mask> {
        self.mask.as_deref()
    }

    /// Element's filters.
    pub fn filters(&self) -> &[Arc<filter::Filter>] {
        &self.filters
    }

    /// Element's object bounding box.
    ///
    /// `objectBoundingBox` in SVG terms. Meaning it doesn't affected by parent transforms.
    ///
    /// Can be set to `None` in case of an empty group.
    pub fn bounding_box(&self) -> Rect {
        self.bounding_box
    }

    /// Element's bounding box in canvas coordinates.
    ///
    /// `userSpaceOnUse` in SVG terms.
    pub fn abs_bounding_box(&self) -> Rect {
        self.abs_bounding_box
    }

    /// Element's object bounding box including stroke.
    ///
    /// Similar to `bounding_box`, but includes stroke.
    pub fn stroke_bounding_box(&self) -> Rect {
        self.stroke_bounding_box
    }

    /// Element's bounding box including stroke in user coordinates.
    ///
    /// Similar to `abs_bounding_box`, but includes stroke.
    pub fn abs_stroke_bounding_box(&self) -> Rect {
        self.abs_stroke_bounding_box
    }

    /// Element's "layer" bounding box in object units.
    ///
    /// Conceptually, this is `stroke_bounding_box` expanded and/or clipped
    /// by `filters_bounding_box`, but also including all the children.
    /// This is the bounding box `svgr` will later use to allocate layers/pixmaps
    /// during isolated groups rendering.
    ///
    /// Only groups have it, because only groups can have filters.
    /// For other nodes layer bounding box is the same as stroke bounding box.
    ///
    /// Unlike other bounding boxes, cannot have zero size.
    pub fn layer_bounding_box(&self) -> NonZeroRect {
        self.layer_bounding_box
    }

    /// Element's "layer" bounding box in canvas units.
    pub fn abs_layer_bounding_box(&self) -> NonZeroRect {
        self.abs_layer_bounding_box
    }

    /// Group's children.
    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// Pre-computed hash for fully static groups.
    ///
    /// If Some, the group is fully static (no runtime-varying content) and
    /// can be cached permanently. This hash was computed at compile-time.
    pub fn static_hash(&self) -> Option<u64> {
        self.static_hash
    }

    /// Checks if this group should be isolated during rendering.
    pub fn should_isolate(&self) -> bool {
        self.isolate
            || self.opacity != Opacity::ONE
            || self.clip_path.is_some()
            || self.mask.is_some()
            || !self.filters.is_empty()
            || self.blend_mode != BlendMode::Normal // TODO: probably not needed?
    }

    /// Returns `true` if the group has any children.
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Calculates a node's filter bounding box.
    ///
    /// Filters with `objectBoundingBox` and missing or zero `bounding_box` would be ignored.
    ///
    /// Note that a filter region can act like a clipping rectangle,
    /// therefore this function can produce a bounding box smaller than `bounding_box`.
    ///
    /// Returns `None` when then group has no filters.
    ///
    /// This function is very fast, that's why we do not store this bbox as a `Group` field.
    pub fn filters_bounding_box(&self) -> Option<NonZeroRect> {
        let mut full_region = BBox::default();
        for filter in &self.filters {
            full_region = full_region.expand(filter.rect);
        }

        full_region.to_non_zero_rect()
    }

    fn subroots(&self, f: &mut dyn FnMut(&Group)) {
        if let Some(ref clip) = self.clip_path {
            f(&clip.root);

            if let Some(ref sub_clip) = clip.clip_path {
                f(&sub_clip.root);
            }
        }

        if let Some(ref mask) = self.mask {
            f(&mask.root);

            if let Some(ref sub_mask) = mask.mask {
                f(&sub_mask.root);
            }
        }

        for filter in &self.filters {
            for primitive in &filter.primitives {
                if let filter::Kind::Image(ref image) = primitive.kind {
                    if let filter::ImageKind::Use(ref use_node) = image.data {
                        f(use_node);
                    }
                }
            }
        }
    }
}

/// Representation of the [`paint-order`] property.
///
/// `usvgr` will handle `markers` automatically,
/// therefore we provide only `fill` and `stroke` variants.
///
/// [`paint-order`]: https://www.w3.org/TR/SVG2/painting.html#PaintOrder
#[derive(Clone, Copy, PartialEq, Debug, Hash, Eq)]
#[allow(missing_docs)]
pub enum PaintOrder {
    FillAndStroke,
    StrokeAndFill,
}

impl Default for PaintOrder {
    fn default() -> Self {
        Self::FillAndStroke
    }
}

/// What a [`FastShape`] is, in its path's user space.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FastShapeKind {
    /// A `circle` or `ellipse`, by its bounding rectangle.
    Ellipse(NonZeroRect),
    /// A `rect` with rounded corners. The radii are already clamped to half the size.
    RoundRect {
        /// The rectangle.
        rect: NonZeroRect,
        /// Horizontal corner radius.
        rx: f32,
        /// Vertical corner radius.
        ry: f32,
    },
}

impl std::hash::Hash for FastShapeKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let (tag, rect, rx, ry) = match *self {
            FastShapeKind::Ellipse(rect) => (0u8, rect, 0.0, 0.0),
            FastShapeKind::RoundRect { rect, rx, ry } => (1u8, rect, rx, ry),
        };
        tag.hash(state);
        for value in [rect.x(), rect.y(), rect.width(), rect.height(), rx, ry] {
            value.to_bits().hash(state);
        }
    }
}

/// A basic shape a renderer can draw natively: `circle`, `ellipse` or a rounded `rect`.
///
/// Only produced with [`Options::fast_shapes`](crate::Options::fast_shapes), for renderers that
/// draw [`FastShape::kind`] natively with the fill, stroke and transform of [`FastShape::path`].
/// The outline is not built during conversion; renderers that need it call
/// [`FastShape::to_path`].
#[derive(Clone, Debug, Hash)]
pub struct FastShape {
    pub(crate) kind: FastShapeKind,
    pub(crate) path: Path,
}

impl FastShape {
    /// The shape's geometry.
    pub fn kind(&self) -> FastShapeKind {
        self.kind
    }

    /// Fill, stroke, transform and bounding boxes of the shape.
    ///
    /// Its [`Path::data`] is only the bounding rectangle, not the outline.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// [`FastShape::path`] with the real outline as its data, for renderers that do not draw
    /// fast shapes natively.
    pub fn to_path(&self) -> Option<Path> {
        let mut path = self.path.clone();
        path.data = Arc::new(self.kind.to_path()?);
        Some(path)
    }
}

impl FastShapeKind {
    /// The rectangle the shape fills.
    pub fn rect(&self) -> NonZeroRect {
        match *self {
            FastShapeKind::Ellipse(rect) | FastShapeKind::RoundRect { rect, .. } => rect,
        }
    }

    /// The outline as cubic curves, starting where the SVG spec's path for the element starts
    /// and going clockwise, so dashes line up.
    pub fn to_path(&self) -> Option<tiny_skia_path::Path> {
        match *self {
            FastShapeKind::Ellipse(rect) => {
                round_rect_outline(rect, rect.width() / 2.0, rect.height() / 2.0, true)
            }
            FastShapeKind::RoundRect { rect, rx, ry } => round_rect_outline(rect, rx, ry, false),
        }
    }
}

/// A rect with elliptical corners: one cubic per corner, joined by the straight edges that are
/// left. Starts where the SVG spec's decomposition starts, at the end of the top-left corner for
/// a `rect` and at the rightmost point for a `circle`/`ellipse`, and goes clockwise.
fn round_rect_outline(
    rect: NonZeroRect,
    rx: f32,
    ry: f32,
    start_at_right: bool,
) -> Option<tiny_skia_path::Path> {
    // 4/3 * (sqrt(2) - 1): a cubic through the quarter arc's ends and midpoint.
    const KAPPA: f32 = 0.552_284_8;
    let (kx, ky) = (rx * (1.0 - KAPPA), ry * (1.0 - KAPPA));
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());

    // Each corner: where it starts, its two control points, where it ends.
    let top_right = [(r - rx, t), (r - kx, t), (r, t + ky), (r, t + ry)];
    let bottom_right = [(r, b - ry), (r, b - ky), (r - kx, b), (r - rx, b)];
    let bottom_left = [(l + rx, b), (l + kx, b), (l, b - ky), (l, b - ry)];
    let top_left = [(l, t + ry), (l, t + ky), (l + kx, t), (l + rx, t)];
    let (start, corners) = if start_at_right {
        (
            top_right[3],
            [bottom_right, bottom_left, top_left, top_right],
        )
    } else {
        (
            top_left[3],
            [top_right, bottom_right, bottom_left, top_left],
        )
    };

    // Edges shorter than this are where two corners meet, as in a circle or a pill.
    let epsilon = (rect.width() + rect.height()) * 1e-6;
    let mut builder = tiny_skia_path::PathBuilder::with_capacity(10, 17);
    builder.move_to(start.0, start.1);
    let mut current = start;
    for [from, c1, c2, to] in corners {
        if (from.0 - current.0).abs() > epsilon || (from.1 - current.1).abs() > epsilon {
            builder.line_to(from.0, from.1);
        }
        builder.cubic_to(c1.0, c1.1, c2.0, c2.1, to.0, to.1);
        current = to;
    }
    builder.close();
    builder.finish()
}

/// A path element.
#[derive(Clone, Debug)]
pub struct Path {
    pub(crate) id: String,
    pub(crate) visibility: Visibility,
    pub(crate) fill: Option<Fill>,
    pub(crate) stroke: Option<Stroke>,
    pub(crate) paint_order: PaintOrder,
    pub(crate) rendering_mode: ShapeRendering,
    pub(crate) data: Arc<tiny_skia_path::Path>,
    pub(crate) abs_transform: Transform,
    pub(crate) bounding_box: Rect,
    pub(crate) abs_bounding_box: Rect,
    pub(crate) stroke_bounding_box: Rect,
    pub(crate) abs_stroke_bounding_box: Rect,
    /// Pre-computed static hash for cache optimization.
    /// If Some, this path's content is known at compile-time and can be cached permanently.
    pub(crate) static_hash: Option<u64>,
}

impl std::hash::Hash for Path {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.visibility.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.paint_order.hash(state);
        self.rendering_mode.hash(state);
        self.data.custom_hash(state);
        self.abs_transform.custom_hash(state);
        self.bounding_box.custom_hash(state);
        self.abs_bounding_box.custom_hash(state);
        self.stroke_bounding_box.custom_hash(state);
        self.abs_stroke_bounding_box.custom_hash(state);
    }
}

impl Path {
    pub(crate) fn new_simple(data: Arc<tiny_skia_path::Path>) -> Option<Self> {
        Self::new(
            String::new(),
            Visibility::default(),
            None,
            None,
            PaintOrder::default(),
            ShapeRendering::default(),
            data,
            Transform::default(),
            None, // static_hash
        )
    }

    pub(crate) fn new(
        id: String,
        visibility: Visibility,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        paint_order: PaintOrder,
        rendering_mode: ShapeRendering,
        data: Arc<tiny_skia_path::Path>,
        abs_transform: Transform,
        static_hash: Option<u64>,
    ) -> Option<Self> {
        let bounding_box = data.compute_tight_bounds()?;
        let stroke_bounding_box =
            Path::calculate_stroke_bbox(stroke.as_ref(), &data).unwrap_or(bounding_box);

        let abs_bounding_box: Rect;
        let abs_stroke_bounding_box: Rect;
        if abs_transform.has_skew() {
            // TODO: avoid re-alloc
            let path2 = data.as_ref().clone();
            let path2 = path2.transform(abs_transform)?;
            abs_bounding_box = path2.compute_tight_bounds()?;
            abs_stroke_bounding_box =
                Path::calculate_stroke_bbox(stroke.as_ref(), &path2).unwrap_or(abs_bounding_box);
        } else {
            // A transform without a skew can be performed just on a bbox.
            abs_bounding_box = bounding_box.fast_transform(abs_transform)?;
            abs_stroke_bounding_box = stroke_bounding_box.fast_transform(abs_transform)?;
        }

        Some(Path {
            id,
            visibility,
            fill,
            stroke,
            paint_order,
            rendering_mode,
            data,
            abs_transform,
            bounding_box,
            abs_bounding_box,
            stroke_bounding_box,
            abs_stroke_bounding_box,
            static_hash,
        })
    }

    /// The path of a [`FastShape`]: bounding boxes come from the shape's rectangle and the
    /// outline is only built under a skewing transform. `data` is the bounding rectangle.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_fast_shape(
        id: String,
        visibility: Visibility,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        paint_order: PaintOrder,
        rendering_mode: ShapeRendering,
        kind: FastShapeKind,
        abs_transform: Transform,
        static_hash: Option<u64>,
    ) -> Option<Self> {
        let bounding_box = kind.rect().to_rect();
        // Ovals and rounded rects are smooth and closed: no join or cap reaches past half the
        // stroke width.
        let half = stroke
            .as_ref()
            .map_or(0.0, |stroke| stroke.width.get() / 2.0);
        let stroke_bounding_box = Rect::from_ltrb(
            bounding_box.left() - half,
            bounding_box.top() - half,
            bounding_box.right() + half,
            bounding_box.bottom() + half,
        )?;

        let (abs_bounding_box, abs_stroke_bounding_box) = if abs_transform.has_skew() {
            let outline = kind.to_path()?.transform(abs_transform)?;
            let abs_bounding_box = outline.compute_tight_bounds()?;
            let abs_stroke_bounding_box =
                Path::calculate_stroke_bbox(stroke.as_ref(), &outline).unwrap_or(abs_bounding_box);
            (abs_bounding_box, abs_stroke_bounding_box)
        } else {
            (
                bounding_box.fast_transform(abs_transform)?,
                stroke_bounding_box.fast_transform(abs_transform)?,
            )
        };

        Some(Path {
            id,
            visibility,
            fill,
            stroke,
            paint_order,
            rendering_mode,
            data: Arc::new(tiny_skia_path::PathBuilder::from_rect(bounding_box)),
            abs_transform,
            bounding_box,
            abs_bounding_box,
            stroke_bounding_box,
            abs_stroke_bounding_box,
            static_hash,
        })
    }

    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Isn't automatically generated.
    /// Can be empty.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Element visibility.
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// Fill style.
    pub fn fill(&self) -> Option<&Fill> {
        self.fill.as_ref()
    }

    /// Stroke style.
    pub fn stroke(&self) -> Option<&Stroke> {
        self.stroke.as_ref()
    }

    /// Fill and stroke paint order.
    ///
    /// Since markers will be replaced with regular nodes automatically,
    /// `usvgr` doesn't provide the `markers` order type. It's was already done.
    ///
    /// `paint-order` in SVG.
    pub fn paint_order(&self) -> PaintOrder {
        self.paint_order
    }

    /// Rendering mode.
    ///
    /// `shape-rendering` in SVG.
    pub fn rendering_mode(&self) -> ShapeRendering {
        self.rendering_mode
    }

    // TODO: find a better name
    /// Segments list.
    ///
    /// All segments are in absolute coordinates.
    pub fn data(&self) -> &tiny_skia_path::Path {
        self.data.as_ref()
    }

    /// Element's absolute transform.
    ///
    /// Contains all ancestors transforms including elements's transform.
    ///
    /// Note that this is not the relative transform present in SVG.
    /// The SVG one would be set only on groups.
    pub fn abs_transform(&self) -> Transform {
        self.abs_transform
    }

    /// Element's object bounding box.
    ///
    /// `objectBoundingBox` in SVG terms. Meaning it doesn't affected by parent transforms.
    pub fn bounding_box(&self) -> Rect {
        self.bounding_box
    }

    /// Element's bounding box in canvas coordinates.
    ///
    /// `userSpaceOnUse` in SVG terms.
    pub fn abs_bounding_box(&self) -> Rect {
        self.abs_bounding_box
    }

    /// Element's object bounding box including stroke.
    ///
    /// Will have the same value as `bounding_box` when path has no stroke.
    pub fn stroke_bounding_box(&self) -> Rect {
        self.stroke_bounding_box
    }

    /// Element's bounding box including stroke in canvas coordinates.
    ///
    /// Will have the same value as `abs_bounding_box` when path has no stroke.
    pub fn abs_stroke_bounding_box(&self) -> Rect {
        self.abs_stroke_bounding_box
    }

    /// Returns the pre-computed static hash for this path, if any.
    /// This is set at compile-time by svgr-macro for fully static paths.
    pub fn static_hash(&self) -> Option<u64> {
        self.static_hash
    }

    fn calculate_stroke_bbox(stroke: Option<&Stroke>, path: &tiny_skia_path::Path) -> Option<Rect> {
        let mut stroke = stroke?.to_tiny_skia();
        // According to the spec, dash should not be accounted during bbox calculation.
        stroke.dash = None;

        // TODO: avoid for round and bevel caps

        // Expensive, but there is not much we can do about it.
        if let Some(stroked_path) = path.stroke(&stroke, 1.0) {
            return stroked_path.compute_tight_bounds();
        }

        None
    }

    fn subroots(&self, f: &mut dyn FnMut(&Group)) {
        if let Some(Paint::Pattern(ref patt)) = self.fill.as_ref().map(|f| &f.paint) {
            f(patt.root())
        }
        if let Some(Paint::Pattern(ref patt)) = self.stroke.as_ref().map(|f| &f.paint) {
            f(patt.root())
        }
    }
}

/// An embedded image kind.
#[derive(Clone)]
pub enum ImageKind {
    /// Contains preloaded decoded image data
    DATA(Arc<PreloadedImageData>),
    /// A preprocessed SVG tree. Can be rendered as is.
    SVG {
        /// Original href of the image.
        original_href: String,
        /// Parsed and converted SVG tree.
        tree: Arc<Tree>,
    },
}

impl std::fmt::Debug for ImageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ImageKind::DATA(_) => f.write_str("ImageKind::DATA(..)"),
            ImageKind::SVG { .. } => f.write_str("ImageKind::SVG(..)"),
        }
    }
}

/// A raster image element.
///
/// `image` element in SVG.
#[derive(Clone, Debug)]
pub struct Image {
    pub(crate) id: String,
    pub(crate) visibility: Visibility,
    pub(crate) view_box: ViewBox,
    pub(crate) rendering_mode: ImageRendering,
    pub(crate) abs_transform: Transform,
    pub(crate) abs_bounding_box: NonZeroRect,
    /// Used for hasihing to skip hashing of the image data itself
    pub(crate) origin_href: String,
    pub(crate) kind: ImageKind,
}

impl std::hash::Hash for Image {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Do not hash the id and the kind
        self.visibility.hash(state);
        self.view_box.hash(state);
        self.rendering_mode.hash(state);
        self.abs_transform.custom_hash(state);
        self.abs_bounding_box.custom_hash(state);
        self.origin_href.hash(state);
    }
}

impl Image {
    /// Element's ID.
    ///
    /// Taken from the SVG itself.
    /// Isn't automatically generated.
    /// Can be empty.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Element visibility.
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// An image rectangle in which it should be fit.
    ///
    /// Combination of the `x`, `y`, `width`, `height` and `preserveAspectRatio`
    /// attributes.
    pub fn view_box(&self) -> ViewBox {
        self.view_box
    }

    /// Rendering mode.
    ///
    /// `image-rendering` in SVG.
    pub fn rendering_mode(&self) -> ImageRendering {
        self.rendering_mode
    }

    /// Image data.
    pub fn kind(&self) -> &ImageKind {
        &self.kind
    }

    /// Element's absolute transform.
    ///
    /// Contains all ancestors transforms including elements's transform.
    ///
    /// Note that this is not the relative transform present in SVG.
    /// The SVG one would be set only on groups.
    pub fn abs_transform(&self) -> Transform {
        self.abs_transform
    }

    /// Element's object bounding box.
    ///
    /// `objectBoundingBox` in SVG terms. Meaning it doesn't affected by parent transforms.
    pub fn bounding_box(&self) -> Rect {
        self.view_box.rect.to_rect()
    }

    /// Element's bounding box in canvas coordinates.
    ///
    /// `userSpaceOnUse` in SVG terms.
    pub fn abs_bounding_box(&self) -> Rect {
        self.abs_bounding_box.to_rect()
    }

    fn subroots(&self, f: &mut dyn FnMut(&Group)) {
        if let ImageKind::SVG { ref tree, .. } = self.kind {
            f(&tree.root)
        }
    }
}

/// A nodes tree container.
#[allow(missing_debug_implementations)]
#[derive(Clone, Debug)]
pub struct Tree {
    pub(crate) size: Size,
    pub(crate) view_box: ViewBox,
    pub(crate) root: Group,
    pub(crate) linear_gradients: Vec<Arc<LinearGradient>>,
    pub(crate) radial_gradients: Vec<Arc<RadialGradient>>,
    pub(crate) patterns: Vec<Arc<Pattern>>,
    pub(crate) clip_paths: Vec<Arc<ClipPath>>,
    pub(crate) masks: Vec<Arc<Mask>>,
    pub(crate) filters: Vec<Arc<filter::Filter>>,
}

impl Tree {
    /// Image size.
    ///
    /// Size of an image that should be created to fit the SVG.
    ///
    /// `width` and `height` in SVG.
    pub fn size(&self) -> Size {
        self.size
    }

    /// SVG viewbox.
    ///
    /// Specifies which part of the SVG image should be rendered.
    ///
    /// `viewBox` and `preserveAspectRatio` in SVG.
    pub fn view_box(&self) -> ViewBox {
        self.view_box
    }

    /// The root element of the SVG tree.
    pub fn root(&self) -> &Group {
        &self.root
    }

    /// Returns a renderable node by ID.
    ///
    /// If an empty ID is provided, than this method will always return `None`.
    pub fn node_by_id(&self, id: &str) -> Option<&Node> {
        if id.is_empty() {
            return None;
        }

        node_by_id(&self.root, id)
    }

    /// Checks if the current tree has any text nodes.
    pub fn has_text_nodes(&self) -> bool {
        has_text_nodes(&self.root)
    }

    /// Returns a list of all unique [`LinearGradient`]s in the tree.
    pub fn linear_gradients(&self) -> &[Arc<LinearGradient>] {
        &self.linear_gradients
    }

    /// Returns a list of all unique [`RadialGradient`]s in the tree.
    pub fn radial_gradients(&self) -> &[Arc<RadialGradient>] {
        &self.radial_gradients
    }

    /// Returns a list of all unique [`Pattern`]s in the tree.
    pub fn patterns(&self) -> &[Arc<Pattern>] {
        &self.patterns
    }

    /// Returns a list of all unique [`ClipPath`]s in the tree.
    pub fn clip_paths(&self) -> &[Arc<ClipPath>] {
        &self.clip_paths
    }

    /// Returns a list of all unique [`Mask`]s in the tree.
    pub fn masks(&self) -> &[Arc<Mask>] {
        &self.masks
    }

    /// Returns a list of all unique [`Filter`](filter::Filter)s in the tree.
    pub fn filters(&self) -> &[Arc<filter::Filter>] {
        &self.filters
    }

    pub(crate) fn collect_paint_servers(&mut self) {
        loop_over_paint_servers(&self.root, &mut |paint| match paint {
            Paint::Color(_) => {}
            Paint::LinearGradient(lg) => {
                if !self
                    .linear_gradients
                    .iter()
                    .any(|other| Arc::ptr_eq(&lg, other))
                {
                    self.linear_gradients.push(lg.clone());
                }
            }
            Paint::RadialGradient(rg) => {
                if !self
                    .radial_gradients
                    .iter()
                    .any(|other| Arc::ptr_eq(&rg, other))
                {
                    self.radial_gradients.push(rg.clone());
                }
            }
            Paint::Pattern(patt) => {
                if !self.patterns.iter().any(|other| Arc::ptr_eq(&patt, other)) {
                    self.patterns.push(patt.clone());
                }
            }
        });
    }
}

fn node_by_id<'a>(parent: &'a Group, id: &str) -> Option<&'a Node> {
    for child in &parent.children {
        if child.id() == id {
            return Some(child);
        }

        if let Node::Group(ref g) = child {
            if let Some(n) = node_by_id(g, id) {
                return Some(n);
            }
        }
    }

    None
}

fn has_text_nodes(root: &Group) -> bool {
    for node in &root.children {
        if let Node::Text(_) = node {
            return true;
        }

        let mut has_text = false;

        if let Node::Image(ref image) = node {
            if let ImageKind::SVG { ref tree, .. } = image.kind {
                if has_text_nodes(&tree.root) {
                    has_text = true;
                }
            }
        }

        node.subroots(|subroot| has_text |= has_text_nodes(subroot));

        if has_text {
            return true;
        }
    }

    false
}

fn loop_over_paint_servers(parent: &Group, f: &mut dyn FnMut(&Paint)) {
    fn push(paint: Option<&Paint>, f: &mut dyn FnMut(&Paint)) {
        if let Some(paint) = paint {
            f(paint);
        }
    }

    for node in &parent.children {
        match node {
            Node::Group(ref group) => loop_over_paint_servers(group, f),
            Node::Path(ref path) => {
                push(path.fill.as_ref().map(|f| &f.paint), f);
                push(path.stroke.as_ref().map(|f| &f.paint), f);
            }
            Node::FastShape(ref shape) => {
                push(shape.path.fill.as_ref().map(|f| &f.paint), f);
                push(shape.path.stroke.as_ref().map(|f| &f.paint), f);
            }
            Node::Image(_) => {}
            // Flattened text would be used instead.
            Node::Text(_) => {}
        }

        node.subroots(|subroot| loop_over_paint_servers(subroot, f));
    }
}

impl Group {
    pub(crate) fn collect_clip_paths(&self, clip_paths: &mut Vec<Arc<ClipPath>>) {
        for node in self.children() {
            if let Node::Group(ref g) = node {
                if let Some(ref clip) = g.clip_path {
                    if !clip_paths.iter().any(|other| Arc::ptr_eq(&clip, other)) {
                        clip_paths.push(clip.clone());
                    }

                    if let Some(ref sub_clip) = clip.clip_path {
                        if !clip_paths.iter().any(|other| Arc::ptr_eq(&sub_clip, other)) {
                            clip_paths.push(sub_clip.clone());
                        }
                    }
                }
            }

            node.subroots(|subroot| subroot.collect_clip_paths(clip_paths));

            if let Node::Group(ref g) = node {
                g.collect_clip_paths(clip_paths);
            }
        }
    }

    pub(crate) fn collect_masks(&self, masks: &mut Vec<Arc<Mask>>) {
        for node in self.children() {
            if let Node::Group(ref g) = node {
                if let Some(ref mask) = g.mask {
                    if !masks.iter().any(|other| Arc::ptr_eq(&mask, other)) {
                        masks.push(mask.clone());
                    }

                    if let Some(ref sub_mask) = mask.mask {
                        if !masks.iter().any(|other| Arc::ptr_eq(&sub_mask, other)) {
                            masks.push(sub_mask.clone());
                        }
                    }
                }
            }

            node.subroots(|subroot| subroot.collect_masks(masks));

            if let Node::Group(ref g) = node {
                g.collect_masks(masks);
            }
        }
    }

    pub(crate) fn collect_filters(&self, filters: &mut Vec<Arc<filter::Filter>>) {
        for node in self.children() {
            if let Node::Group(ref g) = node {
                for filter in g.filters() {
                    if !filters.iter().any(|other| Arc::ptr_eq(&filter, other)) {
                        filters.push(filter.clone());
                    }
                }
            }

            node.subroots(|subroot| subroot.collect_filters(filters));

            if let Node::Group(ref g) = node {
                g.collect_filters(filters);
            }
        }
    }

    pub(crate) fn calculate_object_bbox(&mut self) -> Option<NonZeroRect> {
        let mut bbox = BBox::default();
        for child in &self.children {
            let mut c_bbox = child.bounding_box();
            if let Node::Group(ref group) = child {
                if let Some(r) = c_bbox.fast_transform(group.transform) {
                    c_bbox = r;
                }
            }

            bbox = bbox.expand(c_bbox);
        }

        bbox.to_non_zero_rect()
    }

    pub(crate) fn calculate_bounding_boxes(&mut self) -> Option<()> {
        let mut bbox = BBox::default();
        let mut abs_bbox = BBox::default();
        let mut stroke_bbox = BBox::default();
        let mut abs_stroke_bbox = BBox::default();
        let mut layer_bbox = BBox::default();
        for child in &self.children {
            {
                let mut c_bbox = child.bounding_box();
                if let Node::Group(ref group) = child {
                    if let Some(r) = c_bbox.fast_transform(group.transform) {
                        c_bbox = r;
                    }
                }

                bbox = bbox.expand(c_bbox);
            }

            abs_bbox = abs_bbox.expand(child.abs_bounding_box());

            {
                let mut c_bbox = child.stroke_bounding_box();
                if let Node::Group(ref group) = child {
                    if let Some(r) = c_bbox.fast_transform(group.transform) {
                        c_bbox = r;
                    }
                }

                stroke_bbox = stroke_bbox.expand(c_bbox);
            }

            abs_stroke_bbox = abs_stroke_bbox.expand(child.abs_stroke_bounding_box());

            if let Node::Group(ref group) = child {
                let r = group.layer_bounding_box;
                if let Some(r) = r.fast_transform(group.transform) {
                    layer_bbox = layer_bbox.expand(r);
                }
            } else {
                // Not a group - no need to transform.
                layer_bbox = layer_bbox.expand(child.stroke_bounding_box());
            }
        }

        // `bbox` can be None for empty groups, but we still have to
        // calculate `layer_bounding_box after` it.
        if let Some(bbox) = bbox.to_rect() {
            self.bounding_box = bbox;
            self.abs_bounding_box = abs_bbox.to_rect()?;
            self.stroke_bounding_box = stroke_bbox.to_rect()?;
            self.abs_stroke_bounding_box = abs_stroke_bbox.to_rect()?;
        }

        // Filter bbox has a higher priority than layers bbox.
        if let Some(filter_bbox) = self.filters_bounding_box() {
            self.layer_bounding_box = filter_bbox;
        } else {
            self.layer_bounding_box = layer_bbox.to_non_zero_rect()?;
        }

        self.abs_layer_bounding_box = self.layer_bounding_box.fast_transform(self.abs_transform)?;

        Some(())
    }
}

/// Allocation free replacements for `Rect::transform` and `NonZeroRect::transform`,
/// which build and transform a whole path just to transform four corners. The result
/// is computed with exactly the same arithmetic.
pub(crate) trait FastTransform: Sized {
    fn fast_transform(&self, ts: Transform) -> Option<Self>;
}

impl FastTransform for Rect {
    #[inline]
    fn fast_transform(&self, ts: Transform) -> Option<Self> {
        if ts.is_identity() {
            return Some(*self);
        }

        let mut points = [
            tiny_skia_path::Point::from_xy(self.left(), self.top()),
            tiny_skia_path::Point::from_xy(self.right(), self.top()),
            tiny_skia_path::Point::from_xy(self.right(), self.bottom()),
            tiny_skia_path::Point::from_xy(self.left(), self.bottom()),
        ];
        ts.map_points(&mut points);
        Rect::from_points(&points)
    }
}

impl FastTransform for NonZeroRect {
    #[inline]
    fn fast_transform(&self, ts: Transform) -> Option<Self> {
        if ts.is_identity() {
            return Some(*self);
        }

        self.to_rect().fast_transform(ts)?.to_non_zero_rect()
    }
}
