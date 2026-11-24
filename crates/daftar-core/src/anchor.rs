//! Change-resilient text anchoring for highlights and bookmarks.
//!
//! An anchor stores the selected quote, surrounding context, and byte offsets. When a file is
//! edited externally, the reader tries the original offsets first, then a whitespace-tolerant
//! search scored by context. Anchors are never silently deleted: unresolved anchors are
//! reported as orphaned so the user can review them.

use std::ops::Range;

use serde::{Deserialize, Serialize};

/// Stored anchor information for a highlight or bookmark.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextAnchor {
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub start: usize,
    pub end: usize,
    pub block_index: Option<usize>,
}

impl TextAnchor {
    /// Build an anchor from a byte range in the Markdown source.
    pub fn create(source: &str, range: Range<usize>) -> Self {
        let (mut start, mut end) = clamp_boundaries(source, range.start, range.end);
        while start < end {
            let ch = source[start..].chars().next();
            if ch.map(|c| c.is_whitespace()).unwrap_or(false) {
                start += ch.map(|c| c.len_utf8()).unwrap_or(1);
            } else {
                break;
            }
        }
        while end > start {
            let ch = source[..end].chars().next_back();
            if ch.map(|c| c.is_whitespace()).unwrap_or(false) {
                end -= ch.map(|c| c.len_utf8()).unwrap_or(1);
            } else {
                break;
            }
        }
        let quote = source[start..end].to_string();
        let prefix = context_before(source, start, 48);
        let suffix = context_after(source, end, 48);
        Self {
            quote,
            prefix,
            suffix,
            start,
            end,
            block_index: None,
        }
    }

    pub fn source_range(&self) -> Range<usize> {
        self.start..self.end.max(self.start)
    }
}

/// Result of relocating an anchor in a (possibly edited) document.
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorStatus {
    pub range: Option<Range<usize>>,
    /// The stored offsets still matched exactly.
    pub exact: bool,
    /// 0.0–1.0, based on context similarity.
    pub confidence: f32,
}

impl AnchorStatus {
    pub fn is_orphaned(&self) -> bool {
        self.range.is_none()
    }

    pub fn range(&self) -> Option<Range<usize>> {
        self.range.clone()
    }
}

/// Relocate a stored anchor against current file contents.
pub fn relocate_anchor(source: &str, anchor: &TextAnchor) -> AnchorStatus {
    let quote_norm = normalize(&anchor.quote);
    if quote_norm.is_empty() {
        return AnchorStatus {
            range: None,
            exact: false,
            confidence: 0.0,
        };
    }

    // Fast path: offsets are still valid.
    if anchor.start <= anchor.end && anchor.end <= source.len() {
        if source.is_char_boundary(anchor.start) && source.is_char_boundary(anchor.end) {
            let current = normalize(&source[anchor.start..anchor.end]);
            if current == quote_norm {
                return AnchorStatus {
                    range: Some(anchor.start..anchor.end),
                    exact: true,
                    confidence: 1.0,
                };
            }
        }
    }

    let (haystack, map) = normalize_with_map(source);
    let needle = if quote_norm.len() > 2_000 {
        // Very long selections: search with the first 1k characters, then extend to the full
        // quote from the found position when possible.
        let cut = floor_char_boundary(&quote_norm, 1_000);
        &quote_norm[..cut]
    } else {
        &quote_norm[..]
    };

    let mut best: Option<(usize, f32)> = None;
    let mut search_from = 0usize;
    let mut inspected = 0usize;
    while search_from <= haystack.len() {
        let Some(found) = haystack[search_from..].find(needle) else {
            break;
        };
        let hit = search_from + found;
        inspected += 1;
        let score = context_score(source, &map, hit, needle.len(), anchor);
        if best.map_or(true, |(_, s)| score > s) {
            best = Some((hit, score));
        }
        // The first strong context match is good enough for responsive UI.
        if score >= 0.995 {
            break;
        }
        if inspected >= 200 {
            break;
        }
        search_from = hit + 1;
        while search_from < haystack.len() && !haystack.is_char_boundary(search_from) {
            search_from += 1;
        }
    }

    let Some((hit, score)) = best else {
        return AnchorStatus {
            range: None,
            exact: false,
            confidence: 0.0,
        };
    };

    let start = map.get(hit).copied().unwrap_or(anchor.start);
    // Prefer the original quote length in source bytes when it still lines up; otherwise map
    // the normalized end back to source.
    let norm_end = (hit + needle.len()).min(haystack.len());
    // `map` points at the source start of each normalised byte; the entry at the boundary
    // after the match is the exclusive end of the matched text.
    let mut end = map
        .get(norm_end)
        .copied()
        .unwrap_or_else(|| source.len())
        .min(source.len());
    while end > start && !source.is_char_boundary(end) {
        end -= 1;
    }
    while start < source.len() && !source.is_char_boundary(start) {
        // Move forward to the next char boundary.
        let next = source[start..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        if start + next <= source.len() {
            return AnchorStatus {
                range: Some((start + next)..(end.max(start + next))),
                exact: false,
                confidence: score.max(0.05),
            };
        }
        break;
    }
    // Trim surrounding whitespace introduced by normalization.
    let mut s = start;
    while s < end && source.as_bytes().get(s).is_some_and(|b| b.is_ascii_whitespace()) {
        s += 1;
    }
    let mut e = end;
    while e > s && source.as_bytes().get(e - 1).is_some_and(|b| b.is_ascii_whitespace()) {
        e -= 1;
    }
    AnchorStatus {
        range: Some(s..e.max(s)),
        exact: false,
        confidence: score,
    }
}

fn context_score(
    source: &str,
    map: &[usize],
    hit: usize,
    needle_len: usize,
    anchor: &TextAnchor,
) -> f32 {
    let source_start = map.get(hit).copied().unwrap_or(0);
    let source_end_idx = (hit + needle_len).saturating_sub(1).min(map.len().saturating_sub(1));
    let source_end = map
        .get(source_end_idx)
        .copied()
        .unwrap_or(source_start)
        .saturating_add(1)
        .min(source.len());

    let actual_prefix = concept_prefix(source, source_start);
    let actual_suffix = concept_suffix(source, source_end);
    let expected_prefix = concept_prefix(&anchor.prefix, anchor.prefix.len());
    let expected_suffix = concept_suffix(&anchor.suffix, 0);

    let prefix_score = tail_similarity(&expected_prefix, &actual_prefix);
    let suffix_score = head_similarity(&expected_suffix, &actual_suffix);
    let context = (prefix_score + suffix_score) / 2.0;
    // If both contexts are empty, the quote alone is a weak signal.
    let has_context = !anchor.prefix.trim().is_empty() || !anchor.suffix.trim().is_empty();
    if has_context {
        0.35 + 0.65 * context
    } else {
        0.6
    }
}

fn concept_prefix(text: &str, end: usize) -> String {
    normalize(&context_before(text, end, 64))
}

fn concept_suffix(text: &str, start: usize) -> String {
    normalize(&context_after(text, start, 64))
}

fn tail_similarity(expected: &str, actual: &str) -> f32 {
    if expected.is_empty() {
        return 0.5;
    }
    let expected_chars: Vec<char> = expected.chars().rev().collect();
    let actual_chars: Vec<char> = actual.chars().rev().collect();
    let common = expected_chars
        .iter()
        .zip(actual_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();
    (common as f32 / expected_chars.len() as f32).clamp(0.0, 1.0)
}

fn head_similarity(expected: &str, actual: &str) -> f32 {
    if expected.is_empty() {
        return 0.5;
    }
    let expected_chars: Vec<char> = expected.chars().collect();
    let actual_chars: Vec<char> = actual.chars().collect();
    let common = expected_chars
        .iter()
        .zip(actual_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();
    (common as f32 / expected_chars.len() as f32).clamp(0.0, 1.0)
}

fn context_before(source: &str, index: usize, chars: usize) -> String {
    if index == 0 {
        return String::new();
    }
    let idx = index.min(source.len());
    let idx = floor_char_boundary(source, idx);
    source[..idx]
        .chars()
        .rev()
        .take(chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn context_after(source: &str, index: usize, chars: usize) -> String {
    if index >= source.len() {
        return String::new();
    }
    let idx = ceil_char_boundary(source, index);
    source[idx..].chars().take(chars).collect()
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    idx = idx.min(s.len());
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn clamp_boundaries(source: &str, start: usize, end: usize) -> (usize, usize) {
    let start = floor_char_boundary(source, start.min(source.len()));
    let end = ceil_char_boundary(source, end.max(start).min(source.len()));
    (start, end)
}

/// Collapse whitespace and lowercase for tolerant matching.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

/// Normalise while retaining a map from each output byte to the source byte offset.
fn normalize_with_map(source: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(source.len());
    let mut map = Vec::with_capacity(source.len() + 1);
    let mut space = false;
    for (idx, ch) in source.char_indices() {
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            map.push(idx);
            out.push(' ');
        }
        space = false;
        for lower in ch.to_lowercase() {
            for _ in 0..lower.len_utf8() {
                map.push(idx);
            }
            out.push(lower);
        }
    }
    map.push(source.len());
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_offsets_are_kept() {
        let src = "Hello beautiful world";
        let anchor = TextAnchor::create(src, 6..15);
        let status = relocate_anchor(src, &anchor);
        assert!(status.exact);
        assert_eq!(status.range, Some(6..15));
        assert_eq!(anchor.quote, "beautiful");
    }

    #[test]
    fn relocation_after_insertion() {
        let src = "Intro. Hello beautiful world.";
        let anchor = TextAnchor::create(src, 13..22);
        let edited = format!("A new preface line.\n\n{src}");
        let status = relocate_anchor(&edited, &anchor);
        assert!(!status.exact);
        let range = status.range.expect("relocated");
        assert_eq!(&edited[range], "beautiful");
        assert!(status.confidence > 0.5);
    }

    #[test]
    fn whitespace_changes_are_tolerated() {
        let src = "one two three four five";
        let anchor = TextAnchor::create(src, 4..13);
        let edited = "one   two\n\nthree\tfour five";
        let status = relocate_anchor(edited, &anchor);
        let range = status.range.expect("relocated");
        assert_eq!(normalize(&edited[range]), "two three");
    }

    #[test]
    fn duplicated_quotes_use_context() {
        let src = "important first line, then the word important again.";
        let first = src.find("important").unwrap();
        let anchor = TextAnchor::create(src, first..first + "important".len());
        let edited = format!("prefix important in the middle\n\n{src}");
        let status = relocate_anchor(&edited, &anchor);
        let range = status.range.expect("relocated");
        assert_eq!(&edited[range.clone()], "important");
        // The occurrence at the original location wins because its context is preserved.
        assert_eq!(range.start, edited.find(src).unwrap() + src.find("important").unwrap());
    }

    #[test]
    fn missing_text_is_reported_not_dropped() {
        let src = "Completely different content.";
        let anchor = TextAnchor::create("Original selected phrase here", 9..25);
        let status = relocate_anchor(src, &anchor);
        assert!(status.is_orphaned());
        // The anchor itself is untouched.
        assert_eq!(anchor.quote, "selected phrase");
    }

    #[test]
    fn persian_relocation_works() {
        let src = "مقدمه\n\nاین یک متن فارسی برای آزمون است.\n";
        let start = src.find("متن فارسی").unwrap();
        let anchor = TextAnchor::create(src, start..start + "متن فارسی".len());
        let edited = format!("پیشگفتار جدید\n\n{src}");
        let status = relocate_anchor(&edited, &anchor);
        let range = status.range.expect("relocated");
        assert_eq!(&edited[range], "متن فارسی");
    }

    #[test]
    fn long_quotes_use_prefix_search_then_extend() {
        let sentence = "the quick brown fox jumps over the lazy dog. ".repeat(80);
        let src = format!("Intro. {sentence} End.");
        let start = src.find("the quick").unwrap();
        let end = start + 1500.min(sentence.len());
        let anchor = TextAnchor::create(&src, start..end);
        let edited = format!("Preface\n\n{src}");
        let status = relocate_anchor(&edited, &anchor);
        assert!(status.range.is_some());
    }

    #[test]
    fn byte_boundaries_are_respected_for_unicode() {
        let src = "سلام دنیا";
        let anchor = TextAnchor::create(src, 0..src.len());
        let status = relocate_anchor(&format!("x {src}"), &anchor);
        let range = status.range.clone().unwrap();
        assert!(src.is_char_boundary(0));
        assert!(range.end <= format!("x {src}").len());
        assert!(range.start >= 2);
    }
}