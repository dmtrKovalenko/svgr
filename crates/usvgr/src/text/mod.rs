// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cell::RefCell;
use std::hash::{BuildHasher, Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use fontdb::{Database, ID};
use svgrtypes::FontFamily;

use self::layout::DatabaseExt;
use crate::tree::FastTransform;
use crate::{Font, FontStretch, FontStyle, Text};

mod colr;
mod flatten;

pub(crate) use flatten::GlyphCache;

/// Provides access to the layout of a text node.
pub mod layout;

/// A shorthand for [FontResolver]'s font selection function.
///
/// This function receives a font specification (families + a style, weight,
/// stretch triple) and a font database and should return the ID of the font
/// that shall be used (if any).
///
/// In the basic case, the function will search the existing fonts in the
/// database to find a good match, e.g. via
/// [`Database::query`](fontdb::Database::query). This is what the [default
/// implementation](FontResolver::default_font_selector) does.
///
/// Unlike upstream resvg, the database is passed by shared reference: fonts
/// cannot be loaded dynamically during parsing and must be added to the
/// database passed to `Tree::from_*` upfront.
pub type FontSelectionFn<'a> = Box<dyn Fn(&Font, &Database) -> Option<ID> + Send + Sync + 'a>;

/// A shorthand for [FontResolver]'s fallback selection function.
///
/// This function receives a specific character, a list of already used fonts,
/// and a font database. It should return the ID of a font that
/// - is not any of the already used fonts
/// - is as close as possible to the first already used font (if any)
/// - supports the given character
pub type FallbackSelectionFn<'a> =
    Box<dyn Fn(char, &[ID], &Database) -> Option<ID> + Send + Sync + 'a>;

/// A font resolver for `<text>` elements.
///
/// This type can be useful if you want to have an alternative font handling to
/// the default one. By default, fonts are selected from the font database via
/// [`Database::query`](fontdb::Database::query), and fallback fonts by searching
/// the whole database for a face supporting the missing character.
///
/// Note on caching: the text outline cache (see `Cache::new_with_text_cache`)
/// keys converted text only by the `Text` node itself. Like the font database,
/// the font resolver is assumed to stay the same for the lifetime of a cache.
/// If you use a resolver that can pick different fonts for the same text
/// (or switch resolvers), use a separate cache for it.
pub struct FontResolver<'a> {
    /// Resolver function that will be used when selecting a specific font
    /// for a generic [`Font`] specification.
    pub select_font: FontSelectionFn<'a>,

    /// Resolver function that will be used when selecting a fallback font for a
    /// character.
    pub select_fallback: FallbackSelectionFn<'a>,
}

impl Default for FontResolver<'_> {
    fn default() -> Self {
        FontResolver {
            select_font: FontResolver::default_font_selector(),
            select_fallback: FontResolver::default_fallback_selector(),
        }
    }
}

impl FontResolver<'_> {
    /// Creates a default font selection resolver.
    ///
    /// The default implementation forwards to
    /// [`query`](fontdb::Database::query) on the font database.
    pub fn default_font_selector() -> FontSelectionFn<'static> {
        Box::new(move |font, fontdb| {
            let mut name_list = Vec::new();
            for family in &font.families {
                name_list.push(match family {
                    FontFamily::Serif => fontdb::Family::Serif,
                    FontFamily::SansSerif => fontdb::Family::SansSerif,
                    FontFamily::Cursive => fontdb::Family::Cursive,
                    FontFamily::Fantasy => fontdb::Family::Fantasy,
                    FontFamily::Monospace => fontdb::Family::Monospace,
                    FontFamily::Named(s) => fontdb::Family::Name(s),
                });
            }

            // Use the default font as fallback.
            name_list.push(fontdb::Family::Serif);

            let stretch = match font.stretch {
                FontStretch::UltraCondensed => fontdb::Stretch::UltraCondensed,
                FontStretch::ExtraCondensed => fontdb::Stretch::ExtraCondensed,
                FontStretch::Condensed => fontdb::Stretch::Condensed,
                FontStretch::SemiCondensed => fontdb::Stretch::SemiCondensed,
                FontStretch::Normal => fontdb::Stretch::Normal,
                FontStretch::SemiExpanded => fontdb::Stretch::SemiExpanded,
                FontStretch::Expanded => fontdb::Stretch::Expanded,
                FontStretch::ExtraExpanded => fontdb::Stretch::ExtraExpanded,
                FontStretch::UltraExpanded => fontdb::Stretch::UltraExpanded,
            };

            let style = match font.style {
                FontStyle::Normal => fontdb::Style::Normal,
                FontStyle::Italic => fontdb::Style::Italic,
                FontStyle::Oblique => fontdb::Style::Oblique,
            };

            let query = fontdb::Query {
                families: &name_list,
                weight: fontdb::Weight(font.weight),
                stretch,
                style,
            };

            let id = fontdb.query(&query);
            if id.is_none() {
                log::warn!(
                    "No match for '{}' font-family.",
                    font.families
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            id
        })
    }

    /// Creates a default font fallback selection resolver.
    ///
    /// The default implementation searches through the entire `fontdb`
    /// to find a font that has the correct style and supports the character.
    pub fn default_fallback_selector() -> FallbackSelectionFn<'static> {
        Box::new(|c, exclude_fonts, fontdb| {
            let base_font_id = exclude_fonts[0];

            // Iterate over fonts and check if any of them support the specified char.
            for face in fontdb.faces() {
                // Ignore fonts, that were used for shaping already.
                if exclude_fonts.contains(&face.id) {
                    continue;
                }

                // Last resort fonts (e.g. macOS `LastResort.otf`) map every code point
                // to a "missing glyph" box. Picking one would replace the whole run with
                // boxes, so never use them as a fallback.
                if is_last_resort_font(face) {
                    continue;
                }

                // Check that the new face has the same style.
                let base_face = fontdb.face(base_font_id)?;
                if base_face.style != face.style
                    && base_face.weight != face.weight
                    && base_face.stretch != face.stretch
                {
                    continue;
                }

                if !fontdb.has_char(face.id, c) {
                    continue;
                }

                let base_family = base_face
                    .families
                    .iter()
                    .find(|f| f.1 == fontdb::Language::English_UnitedStates)
                    .unwrap_or(&base_face.families[0]);

                let new_family = face
                    .families
                    .iter()
                    .find(|f| f.1 == fontdb::Language::English_UnitedStates)
                    .unwrap_or(&base_face.families[0]);

                log::warn!("Fallback from {} to {}.", base_family.0, new_family.0);
                return Some(face.id);
            }

            None
        })
    }
}

/// Whether the face is a "last resort" font, which has a placeholder glyph for every code point.
fn is_last_resort_font(face: &fontdb::FaceInfo) -> bool {
    face.post_script_name.contains("LastResort")
        || face.families.iter().any(|(name, _)| name.contains("LastResort"))
}

impl std::fmt::Debug for FontResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FontResolver { .. }")
    }
}

/// Text outline and layoute cache
#[derive(Debug)]
pub struct UsvgrTextOutlineCache {
    cache: RefCell<lru::LruCache<u64, Option<Text>>>,
    hash_builder: ahash::RandomState,
}

impl UsvgrTextOutlineCache {
    /// Creates a new cache with the given size.
    /// If the size is 0 none returned.
    pub fn new(size: usize) -> Option<Self> {
        if size > 0 {
            Some(UsvgrTextOutlineCache {
                cache: RefCell::new(lru::LruCache::new(NonZeroUsize::new(size).unwrap())),
                hash_builder: ahash::RandomState::new(),
            })
        } else {
            None
        }
    }
}

/// Shaped text cache: the glyphs and advances of a string set in one font.
///
/// Shaping (bidi runs, kerning, font fallback) is the costly part of laying text out and
/// does not depend on where the text is drawn. [`text_advance`] and the layout of `<text>`
/// elements read and fill the same cache, so text that is measured before it is rendered,
/// or drawn at a new position on every frame, is shaped once.
///
/// Cloning is cheap and gives another handle to the same cache. Like the text outline
/// cache, it assumes that the font database and the font resolver stay the same:
/// [`clear`](Self::clear) it when fonts change.
#[derive(Clone)]
pub struct UsvgrTextShapeCache {
    cache: Arc<Mutex<lru::LruCache<u64, Arc<[layout::Glyph]>>>>,
    hash_builder: ahash::RandomState,
}

impl std::fmt::Debug for UsvgrTextShapeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsvgrTextShapeCache")
            .field("len", &self.len())
            .finish()
    }
}

impl UsvgrTextShapeCache {
    /// Creates a new cache that keeps up to `size` shaped strings.
    /// If the size is 0 none returned.
    pub fn new(size: usize) -> Option<Self> {
        Some(UsvgrTextShapeCache {
            cache: Arc::new(Mutex::new(lru::LruCache::new(NonZeroUsize::new(size)?))),
            hash_builder: ahash::RandomState::new(),
        })
    }

    /// Number of shaped strings in the cache.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the cache holds no shaped strings.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops every shaped string. Call it after the available fonts change.
    pub fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, lru::LruCache<u64, Arc<[layout::Glyph]>>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn key(&self, shaping: impl Hash) -> u64 {
        self.hash_builder.hash_one(shaping)
    }

    pub(crate) fn get(&self, key: u64) -> Option<Arc<[layout::Glyph]>> {
        self.lock().get(&key).cloned()
    }

    pub(crate) fn put(&self, key: u64, glyphs: Arc<[layout::Glyph]>) {
        self.lock().put(key, glyphs);
    }
}

/// Advance width of `text` set in `font` at `font_size`, in the same units as the font size.
///
/// This is the width the text takes when a `<text>` element with default spacing renders it:
/// it runs the shaping that the layout runs (bidi runs, kerning, font variations, automatic
/// optical sizing and fallback fonts) and nothing else. No tree is built and no glyph is
/// outlined.
///
/// With a `cache`, the shaped glyphs are kept, and a later layout of the same text with a
/// [`Cache`](crate::Cache) that holds the same shape cache reuses them.
///
/// Returns `None` when no font matches.
pub fn text_advance(
    text: &str,
    font: &Font,
    font_size: f32,
    resolver: &FontResolver,
    fontdb: &fontdb::Database,
    cache: Option<&UsvgrTextShapeCache>,
) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }

    let font_id = (resolver.select_font)(font, fontdb)?;
    let glyphs = layout::shape_text(
        text,
        font_id,
        || fontdb.load_font(font_id).map(Arc::new),
        false,
        true,
        &font.variations,
        font_size,
        crate::FontOpticalSizing::Auto,
        resolver,
        fontdb,
        cache,
    )?;

    Some(
        glyphs
            .iter()
            .map(|glyph| glyph.width as f32 * glyph.font.scale(font_size))
            .sum(),
    )
}

pub(crate) fn convert_with_cache(
    text: Text,
    resolver: &FontResolver,
    fontdb: &fontdb::Database,
    cache: Option<&UsvgrTextOutlineCache>,
    shape_cache: Option<&UsvgrTextShapeCache>,
    glyph_cache: &mut GlyphCache,
) -> Option<Text> {
    match cache {
        Some(UsvgrTextOutlineCache {
            cache,
            hash_builder,
        }) => {
            let mut hasher = hash_builder.build_hasher();
            text.hash(&mut hasher);
            let hash = hasher.finish();

            cache
                .borrow_mut()
                .get_or_insert(hash, || {
                    convert(text, resolver, fontdb, shape_cache, glyph_cache)
                })
                // TODO figure out if we can avoid cloning here
                // it is pretty expensive but in order to convert his to Rc
                // it needs to remove all the mutabalities around flattened
                .clone()
        }
        None => convert(text, resolver, fontdb, shape_cache, glyph_cache),
    }
}

/// Convert a text into its paths. This is done in two steps:
/// 1. We convert the text into glyphs and position them according to the rules specified in the
/// SVG specifiation. While doing so, we also calculate the text bbox (which is not based on the
/// outlines of a glyph, but instead the glyph metrics as well as decoration spans).
/// 2. We convert all of the positioned glyphs into outlines.
pub(crate) fn convert(
    mut text: Text,
    resolver: &FontResolver,
    fontdb: &fontdb::Database,
    shape_cache: Option<&UsvgrTextShapeCache>,
    glyph_cache: &mut GlyphCache,
) -> Option<Text> {
    let (text_fragments, bbox) = layout::layout_text(&text, resolver, fontdb, shape_cache)?;
    text.layouted = text_fragments;
    text.bounding_box = bbox.to_rect();
    text.abs_bounding_box = bbox.fast_transform(text.abs_transform)?.to_rect();

    let (group, stroke_bbox) = flatten::flatten(&mut text, fontdb, glyph_cache)?;
    text.flattened = Box::new(group);
    text.stroke_bounding_box = stroke_bbox.to_rect();
    text.abs_stroke_bounding_box = stroke_bbox.fast_transform(text.abs_transform)?.to_rect();

    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cache, Node, Options, Tree};

    const SIZE: f32 = 40.0;

    fn fontdb() -> Database {
        let mut fontdb = Database::new();
        fontdb.load_fonts_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../svgr/tests/fonts"));
        fontdb
    }

    fn font(family: &str, weight: u16) -> Font {
        Font::new(
            vec![FontFamily::Named(family.to_string())],
            FontStyle::Normal,
            FontStretch::Normal,
            weight,
            Vec::new(),
        )
    }

    fn svg(family: &str, weight: u16, anchors: &[&str], text: &str) -> String {
        let texts: String = anchors
            .iter()
            .map(|anchor| {
                format!(
                    r#"<text text-anchor="{anchor}" xml:space="preserve" font-family="{family}" font-size="{SIZE}" font-weight="{weight}">{text}</text>"#
                )
            })
            .collect();
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1">{texts}</svg>"#)
    }

    fn parse(svg: &str, fontdb: &Database, cache: &mut Cache) -> Tree {
        let doc = roxmltree::Document::parse(svg).unwrap();
        Tree::from_xmltree_with_cache(&doc, &Options::default(), cache, fontdb).unwrap()
    }

    /// The advance the layout gives the text: every glyph moves by exactly that much
    /// between a start-anchored and an end-anchored copy.
    fn laid_out_advance(fontdb: &Database, family: &str, weight: u16, text: &str) -> f32 {
        let tree = parse(
            &svg(family, weight, &["start", "end"], text),
            fontdb,
            &mut Cache::default(),
        );
        let first_glyph_x: Vec<f32> = tree
            .root()
            .children()
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.layouted()[0].positioned_glyphs[0].transform().tx),
                _ => None,
            })
            .collect();
        first_glyph_x[0] - first_glyph_x[1]
    }

    fn advance(fontdb: &Database, family: &str, weight: u16, text: &str) -> f32 {
        let resolver = FontResolver::default();
        text_advance(text, &font(family, weight), SIZE, &resolver, fontdb, None).unwrap()
    }

    #[test]
    fn advance_matches_the_layout() {
        let fontdb = fontdb();
        for (family, weight, text) in [
            ("Noto Sans", 400, "AVAVAVAV Typography"),
            ("Noto Sans", 700, "To Wave, Yo."),
            // Arabic is not in Noto Sans, so it comes from a fallback font.
            ("Noto Sans", 400, "سلام"),
            ("Noto Sans", 400, "AV سلام AV"),
            ("Amiri", 400, "اَلسَّلَامُ"),
            ("Roboto Flex", 100, "Hamburgefonts"),
            ("Roboto Flex", 900, "Hamburgefonts"),
        ] {
            let measured = advance(&fontdb, family, weight, text);
            let laid_out = laid_out_advance(&fontdb, family, weight, text);
            assert!(
                (measured - laid_out).abs() < 0.01,
                "{family} {weight} {text:?}: measured {measured}, laid out {laid_out}"
            );
        }
    }

    #[test]
    fn advance_includes_kerning() {
        let fontdb = fontdb();
        let pair = advance(&fontdb, "Noto Sans", 400, "AV");
        let separate =
            advance(&fontdb, "Noto Sans", 400, "A") + advance(&fontdb, "Noto Sans", 400, "V");
        assert!(pair < separate - 0.5, "pair {pair}, separate {separate}");
    }

    #[test]
    fn advance_follows_the_weight_axis() {
        let fontdb = fontdb();
        let thin = advance(&fontdb, "Roboto Flex", 100, "Hamburgefonts");
        let black = advance(&fontdb, "Roboto Flex", 900, "Hamburgefonts");
        assert!(black > thin + 1.0, "thin {thin}, black {black}");
    }

    #[test]
    fn advance_of_unknown_family_and_empty_text() {
        let fontdb = Database::new();
        let resolver = FontResolver::default();
        let font = font("Noto Sans", 400);
        assert_eq!(
            text_advance("", &font, SIZE, &resolver, &fontdb, None),
            Some(0.0)
        );
        assert_eq!(
            text_advance("text", &font, SIZE, &resolver, &fontdb, None),
            None
        );
    }

    #[test]
    fn layout_reuses_measured_shapes() {
        let fontdb = fontdb();
        let resolver = FontResolver::default();
        let shapes = UsvgrTextShapeCache::new(16).unwrap();
        let font = font("Noto Sans", 400);

        let measure = |cache| text_advance("Shaped once", &font, SIZE, &resolver, &fontdb, cache);
        let cached = measure(Some(&shapes));
        assert_eq!(shapes.len(), 1);
        assert_eq!(cached, measure(None));
        assert_eq!(cached, measure(Some(&shapes)));
        assert_eq!(shapes.len(), 1);

        let mut cache = Cache::new_with_text_caches(0, shapes.clone());
        parse(
            &svg("Noto Sans", 400, &["start"], "Shaped once"),
            &fontdb,
            &mut cache,
        );
        assert_eq!(shapes.len(), 1, "the layout shaped measured text again");

        parse(
            &svg("Noto Sans", 400, &["start"], "Shaped by the layout"),
            &fontdb,
            &mut cache,
        );
        assert_eq!(shapes.len(), 2);

        shapes.clear();
        assert!(shapes.is_empty());
    }
}
