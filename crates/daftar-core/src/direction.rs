//! Text direction detection and resolution.
//!
//! Direction is computed from Unicode bidi character classes rather than by guessing from
//! glyph shapes. This is the same first-strong-character rule used by Unicode UAX #9 (P2/P3),
//! with an explicit RTL fallback when a document begins with neutral content but is mostly RTL.

use serde::{Deserialize, Serialize};
use unicode_bidi::BidiClass;

/// A user-selectable direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Detect from the content, per document.
    #[default]
    Auto,
    Ltr,
    Rtl,
}

impl Direction {
    pub fn label(self) -> &'static str {
        match self {
            Direction::Auto => "Automatic",
            Direction::Ltr => "Left to right",
            Direction::Rtl => "Right to left",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Direction::Auto => "auto",
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "ltr" => Direction::Ltr,
            "rtl" => Direction::Rtl,
            _ => Direction::Auto,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Direction::Auto => Direction::Rtl,
            Direction::Rtl => Direction::Ltr,
            Direction::Ltr => Direction::Auto,
        }
    }
}

/// The direction that should actually be used for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedDirection {
    Ltr,
    Rtl,
}

impl ResolvedDirection {
    pub fn is_rtl(self) -> bool {
        matches!(self, ResolvedDirection::Rtl)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ResolvedDirection::Ltr => "ltr",
            ResolvedDirection::Rtl => "rtl",
        }
    }

    pub fn from_rtl(rtl: bool) -> Self {
        if rtl {
            ResolvedDirection::Rtl
        } else {
            ResolvedDirection::Ltr
        }
    }
}

/// Result of resolving a stored preference against document content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectionResolution {
    pub resolved: ResolvedDirection,
    /// True when the stored preference was `Auto`.
    pub detected: bool,
    /// True when the document contained strong characters from both directions.
    pub mixed: bool,
}

impl DirectionResolution {
    pub fn is_rtl(&self) -> bool {
        self.resolved.is_rtl()
    }
}

/// Detect the base direction using Unicode bidi classes.
///
/// The first strong character wins. If a document begins with neutral text (numbers,
/// punctuation, code), a majority vote over strong characters decides. This avoids
/// right-aligning an English document that merely starts with a bullet or digit.
pub fn detect_direction(text: &str) -> ResolvedDirection {
    const SAMPLE_CHARS: usize = 16_384;
    let sample: String = text.chars().take(SAMPLE_CHARS).collect();

    let mut rtl_strong = 0usize;
    let mut ltr_strong = 0usize;
    for ch in sample.chars() {
        match unicode_bidi::bidi_class(ch) {
            BidiClass::R | BidiClass::AL => {
                rtl_strong += 1;
                if rtl_strong >= 2 && ltr_strong == 0 {
                    // A clear leading RTL run; no need to scan further for the common case.
                    return ResolvedDirection::Rtl;
                }
            }
            BidiClass::L => {
                ltr_strong += 1;
                if ltr_strong >= 2 && rtl_strong == 0 {
                    return ResolvedDirection::Ltr;
                }
            }
            _ => {}
        }
    }
    if rtl_strong > ltr_strong {
        ResolvedDirection::Rtl
    } else {
        ResolvedDirection::Ltr
    }
}

/// Count strong characters in each direction, used for the "mixed content" indicator.
pub fn strong_char_counts(text: &str) -> (usize, usize) {
    let mut ltr = 0;
    let mut rtl = 0;
    for ch in text.chars().take(50_000) {
        match unicode_bidi::bidi_class(ch) {
            BidiClass::R | BidiClass::AL => rtl += 1,
            BidiClass::L => ltr += 1,
            _ => {}
        }
    }
    (ltr, rtl)
}

/// Resolve the effective direction for a document.
pub fn resolve_direction(preference: Direction, text: &str) -> DirectionResolution {
    let detected = detect_direction(text);
    let (ltr, rtl) = strong_char_counts(text);
    let mixed = ltr > 20 && rtl > 20;
    let resolved = match preference {
        Direction::Ltr => ResolvedDirection::Ltr,
        Direction::Rtl => ResolvedDirection::Rtl,
        Direction::Auto => detected,
    };
    DirectionResolution {
        resolved,
        detected: preference == Direction::Auto,
        mixed,
    }
}

/// Choose the base direction for a single paragraph/line based on its first strong character.
/// Used for mixed RTL/LTR blocks where the document base direction should not force alignment.
pub fn paragraph_direction(text: &str) -> ResolvedDirection {
    for ch in text.chars().take(4096) {
        match unicode_bidi::bidi_class(ch) {
            BidiClass::R | BidiClass::AL => return ResolvedDirection::Rtl,
            BidiClass::L => return ResolvedDirection::Ltr,
            _ => {}
        }
    }
    ResolvedDirection::Ltr
}

/// True if a character belongs to a right-to-left script (used for typography affordances).
pub fn is_rtl_char(ch: char) -> bool {
    matches!(
        unicode_bidi::bidi_class(ch),
        BidiClass::R | BidiClass::AL
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_persian() {
        assert_eq!(detect_direction("سلام دنیا"), ResolvedDirection::Rtl);
    }

    #[test]
    fn detects_english() {
        assert_eq!(
            detect_direction("Hello world, this is a test"),
            ResolvedDirection::Ltr
        );
    }

    #[test]
    fn ignores_leading_numbers_and_punctuation() {
        assert_eq!(detect_direction("123. سلام"), ResolvedDirection::Rtl);
        assert_eq!(detect_direction("123. Hello"), ResolvedDirection::Ltr);
    }

    #[test]
    fn override_wins() {
        let res = resolve_direction(Direction::Ltr, "سلام دنیا");
        assert_eq!(res.resolved, ResolvedDirection::Ltr);
        assert!(!res.detected);
    }

    #[test]
    fn mixed_is_reported() {
        let text = format!("{} English text {}", "سلام ".repeat(30), "words ".repeat(30));
        let res = resolve_direction(Direction::Auto, &text);
        assert!(res.mixed);
    }

    #[test]
    fn paragraph_direction_handles_mixed_lines() {
        assert_eq!(
            paragraph_direction("English first, سپس فارسی"),
            ResolvedDirection::Ltr
        );
        assert_eq!(
            paragraph_direction("فارسی اول، then English"),
            ResolvedDirection::Rtl
        );
    }
}