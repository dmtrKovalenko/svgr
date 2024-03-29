// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cell::RefCell;
use std::hash::{BuildHasher, Hash, Hasher};
use std::num::NonZeroUsize;

use crate::tree::FastTransform;
use crate::Text;

mod flatten;

/// Provides access to the layout of a text node.
pub mod layout;

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

pub(crate) fn convert_with_cache(
    text: Text,
    fontdb: &fontdb::Database,
    cache: Option<&UsvgrTextOutlineCache>,
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
                .get_or_insert(hash, || convert(text, fontdb))
                // TODO figure out if we can avoid cloning here
                // it is pretty expensive but in order to convert his to Rc
                // it needs to remove all the mutabalities around flattened
                .clone()
        }
        None => convert(text, fontdb),
    }
}

/// Convert a text into its paths. This is done in two steps:
/// 1. We convert the text into glyphs and position them according to the rules specified in the
/// SVG specifiation. While doing so, we also calculate the text bbox (which is not based on the
/// outlines of a glyph, but instead the glyph metrics as well as decoration spans).
/// 2. We convert all of the positioned glyphs into outlines.
pub(crate) fn convert(mut text: Text, fontdb: &fontdb::Database) -> Option<Text> {
    let (text_fragments, bbox) = layout::layout_text(&text, fontdb)?;
    text.layouted = text_fragments;
    text.bounding_box = bbox.to_rect();
    text.abs_bounding_box = bbox.fast_transform(text.abs_transform)?.to_rect();

    let (group, stroke_bbox) = flatten::flatten(&mut text, fontdb)?;
    text.flattened = Box::new(group);
    text.stroke_bounding_box = stroke_bbox.to_rect();
    text.abs_stroke_bounding_box = stroke_bbox.fast_transform(text.abs_transform)?.to_rect();

    Some(text)
}
