//! Offset arithmetic between Rust byte offsets, the UTF-16 code-unit offsets
//! Flutter's text input protocol speaks, and the grapheme clusters a caret
//! actually moves in.
//!
//! Dart strings are UTF-16, so every offset in a `TextEditingValue` — the
//! selection, the composing range — counts UTF-16 code units, while Rust
//! strings are UTF-8 and index in bytes. A character outside the Basic
//! Multilingual Plane (an emoji, say) is one `char`, two UTF-16 code units and
//! four UTF-8 bytes, so the two offset spaces only agree on text that stays
//! inside the BMP. Everything here converts Dart's offsets into ours, and
//! reports an offset that does not land on a character boundary instead of
//! panicking or silently misplacing the caret.
//!
//! A code unit is not the unit an *edit* works in either. "e" followed by
//! U+0301 is one thing on screen and one Backspace; so is a flag, a
//! skin-toned hand and a ZWJ family. Those are extended grapheme clusters
//! (UAX #29), and the `*_grapheme_*` helpers are the layer the editing
//! operations step in. They still take and report UTF-16 offsets — that is
//! what goes back over the channel — they just only ever land on offsets that
//! are also cluster boundaries. The plain `*_utf16_*` helpers stay alongside
//! them for the callers that genuinely mean code units: the wire format, and
//! the composing range an IME owns and may well be composing half a cluster
//! of.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// UTF-16 and grapheme-cluster offset arithmetic on string slices.
pub trait StringUtils {
    /// The length of this string in UTF-16 code units.
    fn utf16_len(&self) -> usize;

    /// The byte offset of the UTF-16 code-unit offset `u16_offset`.
    ///
    /// Returns `None` if `u16_offset` is past the end of the string or lands
    /// inside a surrogate pair, i.e. is not a character boundary.
    fn byte_offset_of_utf16(&self, u16_offset: usize) -> Option<usize>;

    /// The byte range of the UTF-16 code-unit range `range`.
    ///
    /// An empty or reversed range collapses to the empty byte range at
    /// `range.start`. Returns `None` unless both ends are character
    /// boundaries of this string.
    fn byte_range_of_utf16(&self, range: Range<usize>) -> Option<Range<usize>>;

    /// `u16_offset` pinned to the character boundary at or before it, and to
    /// the end of the string when it points past it.
    ///
    /// This is how an offset from Dart becomes one of ours when what it
    /// describes is a span of code units rather than an editing position —
    /// the composing range the IME is working on, which may well cut a
    /// cluster in half. For an editing position use
    /// [`floor_grapheme_utf16_offset`](StringUtils::floor_grapheme_utf16_offset).
    fn clamp_utf16_offset(&self, u16_offset: usize) -> usize;

    /// The UTF-16 offsets of this string's extended grapheme-cluster
    /// boundaries, ascending.
    ///
    /// Always starts at `0` and ends at
    /// [`utf16_len`](StringUtils::utf16_len), so the empty string yields a
    /// single `0`.
    fn grapheme_utf16_boundaries(&self) -> impl Iterator<Item = usize>;

    /// The UTF-16 offset of the grapheme-cluster boundary strictly before
    /// `u16_offset`.
    ///
    /// This is the step Backspace and the left arrow take. Returns `None`
    /// only at (or before) the start of the string. An `u16_offset` inside a
    /// cluster resolves to that cluster's start, so a caret the framework has
    /// left mid-cluster deletes the whole cluster rather than splitting it.
    fn prev_grapheme_utf16_offset(&self, u16_offset: usize) -> Option<usize>;

    /// The UTF-16 offset of the grapheme-cluster boundary strictly after
    /// `u16_offset`.
    ///
    /// This is the step Delete and the right arrow take. Returns `None` only
    /// at (or past) the end of the string.
    fn next_grapheme_utf16_offset(&self, u16_offset: usize) -> Option<usize>;

    /// `u16_offset` pinned to the grapheme-cluster boundary at or before it,
    /// and to the end of the string when it points past it.
    ///
    /// Every editing position this module works with goes through here: a
    /// caret between the "e" and the U+0301 of an "é", or inside a flag, is
    /// not somewhere text can be inserted or removed without taking the
    /// cluster apart.
    fn floor_grapheme_utf16_offset(&self, u16_offset: usize) -> usize;

    /// `u16_offset` pinned to the grapheme-cluster boundary at or after it,
    /// and to the end of the string when it points past it.
    ///
    /// The counterpart of
    /// [`floor_grapheme_utf16_offset`](StringUtils::floor_grapheme_utf16_offset)
    /// for a caret that must not move *backwards* — after an insertion, where
    /// flooring would put the caret in front of the text just typed if it
    /// merged into the cluster that follows.
    fn ceil_grapheme_utf16_offset(&self, u16_offset: usize) -> usize;

    /// The UTF-16 offset where the last word starting strictly before
    /// `u16_offset` starts, or `0`.
    ///
    /// A word is a run of clusters that
    /// [count as word clusters](is_word_cluster); whitespace and punctuation
    /// separate them.
    fn prev_word_start_utf16(&self, u16_offset: usize) -> usize;

    /// The UTF-16 offset where the first word ending strictly after
    /// `u16_offset` ends, or [`utf16_len`](StringUtils::utf16_len).
    ///
    /// The mirror of
    /// [`prev_word_start_utf16`](StringUtils::prev_word_start_utf16).
    fn next_word_end_utf16(&self, u16_offset: usize) -> usize;

    /// The UTF-16 ranges of this string's hard line breaks, ascending.
    ///
    /// A break is one grapheme cluster, so `"\r\n"` is a single break: an
    /// offset between the `\r` and the `\n` would put the caret inside a
    /// cluster.
    fn utf16_line_break_ranges(&self) -> impl Iterator<Item = Range<usize>>;
}

/// UTF-16 offset arithmetic that needs to own the string.
pub trait OwnedStringUtils {
    /// Remove the UTF-16 code-unit range `range`.
    ///
    /// Returns `false`, leaving the string untouched, for an empty or
    /// reversed range and for a range that is not a valid range of this
    /// string — which is what a selection that drifted out of step with the
    /// text looks like.
    fn remove_utf16(&mut self, range: Range<usize>) -> bool;
}

/// Whether `cluster` is a hard line break.
///
/// The set is Unicode's mandatory breaks (UAX #14 classes BK, CR, LF and NL),
/// which is what Flutter's text layout treats as the end of a line. A cluster
/// that starts with `\r` is either `"\r"` or `"\r\n"`, and both are one break.
fn is_hard_line_break(cluster: &str) -> bool {
    matches!(
        cluster.chars().next(),
        Some('\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}')
    )
}

/// Whether `cluster` belongs to a word.
///
/// A cluster counts when the character it is built on is alphanumeric; the
/// combining marks, emoji modifiers and joiners that follow do not change
/// that. This is the predicate the `char`-wise scan it replaces used, lifted
/// to clusters so that a word boundary can no longer fall inside one: a
/// combining mark is not alphanumeric, so scanning `char`s stopped between an
/// "e" and its accent. Lifting rather than reaching for UAX #29's own word
/// boundaries keeps the behaviour on text that has no clusters wider than a
/// character — all of ASCII — exactly as it was; UAX #29 would, for instance,
/// treat "foo.bar" as a single word.
fn is_word_cluster(cluster: &str) -> bool {
    cluster.chars().next().is_some_and(char::is_alphanumeric)
}

impl StringUtils for str {
    fn utf16_len(&self) -> usize {
        self.chars().map(char::len_utf16).sum()
    }

    fn byte_offset_of_utf16(&self, u16_offset: usize) -> Option<usize> {
        let mut units = 0;
        for (byte_offset, c) in self.char_indices() {
            if units == u16_offset {
                return Some(byte_offset);
            }
            if units > u16_offset {
                // `u16_offset` named the trailing half of a surrogate pair.
                return None;
            }
            units += c.len_utf16();
        }
        // One past the last character is a valid caret position.
        (units == u16_offset).then_some(self.len())
    }

    fn byte_range_of_utf16(&self, range: Range<usize>) -> Option<Range<usize>> {
        let start = self.byte_offset_of_utf16(range.start)?;
        if range.end <= range.start {
            return Some(start..start);
        }
        Some(start..self.byte_offset_of_utf16(range.end)?)
    }

    fn clamp_utf16_offset(&self, u16_offset: usize) -> usize {
        let mut units = 0;
        for c in self.chars() {
            let next = units + c.len_utf16();
            if next > u16_offset {
                return units;
            }
            units = next;
        }
        units
    }

    fn grapheme_utf16_boundaries(&self) -> impl Iterator<Item = usize> {
        // `0` is a boundary of every string, including the empty one; after
        // that each cluster contributes the boundary at its end.
        std::iter::once(0).chain(self.graphemes(true).scan(0, |units, cluster| {
            *units += cluster.utf16_len();
            Some(*units)
        }))
    }

    fn prev_grapheme_utf16_offset(&self, u16_offset: usize) -> Option<usize> {
        self.grapheme_utf16_boundaries()
            .take_while(|&boundary| boundary < u16_offset)
            .last()
    }

    fn next_grapheme_utf16_offset(&self, u16_offset: usize) -> Option<usize> {
        self.grapheme_utf16_boundaries()
            .find(|&boundary| boundary > u16_offset)
    }

    fn floor_grapheme_utf16_offset(&self, u16_offset: usize) -> usize {
        self.grapheme_utf16_boundaries()
            .take_while(|&boundary| boundary <= u16_offset)
            .last()
            // Unreachable: the first boundary is `0`, which is `<=` any
            // `usize`. Kept so this cannot become the panic it replaces.
            .unwrap_or(0)
    }

    fn ceil_grapheme_utf16_offset(&self, u16_offset: usize) -> usize {
        self.grapheme_utf16_boundaries()
            .find(|&boundary| boundary >= u16_offset)
            .unwrap_or_else(|| self.utf16_len())
    }

    fn prev_word_start_utf16(&self, u16_offset: usize) -> usize {
        let mut units = 0;
        let mut in_word = false;
        let mut last_start = 0;
        for cluster in self.graphemes(true) {
            let start = units;
            units += cluster.utf16_len();
            let is_word = is_word_cluster(cluster);
            if is_word && !in_word {
                // The first cluster of a word run. Offsets only grow from
                // here, so there is nothing left to find.
                if start >= u16_offset {
                    break;
                }
                last_start = start;
            }
            in_word = is_word;
        }
        last_start
    }

    fn next_word_end_utf16(&self, u16_offset: usize) -> usize {
        let mut units = 0;
        // Where the word run currently being walked ends so far, if one is
        // open. A run's end is only settled once the run is over.
        let mut run_end = None;
        for cluster in self.graphemes(true) {
            units += cluster.utf16_len();
            if is_word_cluster(cluster) {
                run_end = Some(units);
            } else if let Some(end) = run_end.take()
                && end > u16_offset
            {
                return end;
            }
        }
        match run_end {
            Some(end) if end > u16_offset => end,
            // `units` is the length of the string in UTF-16 code units.
            _ => units,
        }
    }

    fn utf16_line_break_ranges(&self) -> impl Iterator<Item = Range<usize>> {
        self.graphemes(true)
            .scan(0, |units, cluster| {
                let start = *units;
                *units += cluster.utf16_len();
                Some((start..*units, cluster))
            })
            .filter(|(_, cluster)| is_hard_line_break(cluster))
            .map(|(range, _)| range)
    }
}

impl OwnedStringUtils for String {
    fn remove_utf16(&mut self, range: Range<usize>) -> bool {
        if range.start >= range.end {
            return false;
        }
        let Some(bytes) = self.byte_range_of_utf16(range) else {
            return false;
        };
        self.drain(bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thumbs up, U+1F44D: one char, two UTF-16 units, four UTF-8 bytes.
    const ASTRAL: &str = "\u{1f44d}";
    /// "e" + U+0301 COMBINING ACUTE: two chars, two units, one cluster.
    const ACUTE_E: &str = "e\u{301}";
    /// Thumbs up + U+1F3FD skin tone: two chars, four units, one cluster.
    const SKIN_TONE: &str = "\u{1f44d}\u{1f3fd}";
    /// Man + ZWJ + woman + ZWJ + girl: five chars, eight units, one cluster.
    const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    /// Regional indicators U+1F1FA U+1F1F8: two chars, four units, one
    /// cluster.
    const FLAG: &str = "\u{1f1fa}\u{1f1f8}";

    #[test]
    fn utf16_len_counts_code_units() {
        assert_eq!("".utf16_len(), 0);
        assert_eq!("hello world".utf16_len(), 11);
        // Three BMP chars, three bytes each.
        assert_eq!("日本語".utf16_len(), 3);
        assert_eq!(format!("{ASTRAL}abc").utf16_len(), 5);
        assert_eq!(ACUTE_E.utf16_len(), 2);
        assert_eq!(SKIN_TONE.utf16_len(), 4);
        assert_eq!(FAMILY.utf16_len(), 8);
        assert_eq!(FLAG.utf16_len(), 4);
    }

    #[test]
    fn byte_offset_of_utf16_maps_boundaries() {
        let s = format!("{ASTRAL}abc");
        assert_eq!(s.byte_offset_of_utf16(0), Some(0));
        assert_eq!(s.byte_offset_of_utf16(2), Some(4));
        assert_eq!(s.byte_offset_of_utf16(5), Some(7));
        // Inside the surrogate pair, and past the end.
        assert_eq!(s.byte_offset_of_utf16(1), None);
        assert_eq!(s.byte_offset_of_utf16(6), None);
        assert_eq!("".byte_offset_of_utf16(0), Some(0));
        assert_eq!("".byte_offset_of_utf16(1), None);
    }

    #[test]
    fn byte_range_of_utf16_collapses_reversed_ranges() {
        let s = String::from("hello world");
        assert_eq!(s.byte_range_of_utf16(0..0), Some(0..0));
        assert_eq!(s.byte_range_of_utf16(1..4), Some(1..4));
        // Built through the struct literal because a reversed range literal
        // is a clippy error, and a reversed range is exactly the input under
        // test: a selection whose extent precedes its base.
        assert_eq!(
            s.byte_range_of_utf16(Range { start: 1, end: 0 }),
            Some(1..1)
        );
        assert_eq!(s.byte_range_of_utf16(11..11), Some(11..11));
        // Out of range on either end, rather than silently clamped.
        assert_eq!(s.byte_range_of_utf16(10..12), None);
        assert_eq!(s.byte_range_of_utf16(14..27), None);
    }

    #[test]
    fn byte_range_of_utf16_spans_astral_chars() {
        let s = format!("{ASTRAL}abc");
        assert_eq!(s.byte_range_of_utf16(2..5), Some(4..7));
        assert_eq!(s.byte_range_of_utf16(0..2), Some(0..4));
        // Either end inside the surrogate pair.
        assert_eq!(s.byte_range_of_utf16(1..3), None);
        assert_eq!(s.byte_range_of_utf16(0..1), None);
    }

    #[test]
    fn clamp_utf16_offset_snaps_into_the_string() {
        let s = format!("{ASTRAL}abc");
        assert_eq!(s.clamp_utf16_offset(0), 0);
        assert_eq!(s.clamp_utf16_offset(1), 0);
        assert_eq!(s.clamp_utf16_offset(2), 2);
        assert_eq!(s.clamp_utf16_offset(5), 5);
        assert_eq!(s.clamp_utf16_offset(usize::MAX), 5);
        assert_eq!("".clamp_utf16_offset(7), 0);
        // Code-unit clamping keeps a mid-cluster offset: the composing range
        // an IME owns is a span of code units, not an editing position.
        assert_eq!(ACUTE_E.clamp_utf16_offset(1), 1);
    }

    #[test]
    fn grapheme_boundaries_keep_clusters_whole() {
        let boundaries = |s: &str| -> Vec<usize> { s.grapheme_utf16_boundaries().collect() };
        assert_eq!(boundaries(""), vec![0]);
        assert_eq!(boundaries("abc"), vec![0, 1, 2, 3]);
        // The cluster has a character boundary inside it, at offset 1, which
        // is exactly the offset no caret may land on.
        assert_eq!(ACUTE_E.byte_offset_of_utf16(1), Some(1));
        assert_eq!(boundaries(ACUTE_E), vec![0, 2]);
        assert_eq!(boundaries(SKIN_TONE), vec![0, 4]);
        assert_eq!(boundaries(FAMILY), vec![0, 8]);
        assert_eq!(boundaries(FLAG), vec![0, 4]);
        // Two flags are two clusters, not one run of regional indicators.
        assert_eq!(boundaries(&format!("{FLAG}{FLAG}")), vec![0, 4, 8]);
        // CRLF is one cluster.
        assert_eq!(boundaries("a\r\nb"), vec![0, 1, 3, 4]);
        // Hebrew: BMP, one cluster per char, logical order.
        assert_eq!(boundaries("שלום"), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn grapheme_steps_skip_whole_clusters() {
        let s = format!("a{FAMILY}b");
        assert_eq!(s.next_grapheme_utf16_offset(0), Some(1));
        assert_eq!(s.next_grapheme_utf16_offset(1), Some(9));
        assert_eq!(s.next_grapheme_utf16_offset(9), Some(10));
        assert_eq!(s.next_grapheme_utf16_offset(10), None);
        assert_eq!(s.prev_grapheme_utf16_offset(10), Some(9));
        assert_eq!(s.prev_grapheme_utf16_offset(9), Some(1));
        assert_eq!(s.prev_grapheme_utf16_offset(1), Some(0));
        assert_eq!(s.prev_grapheme_utf16_offset(0), None);
        // An offset inside the family resolves outwards to its edges rather
        // than taking the cluster apart. The character helpers would have
        // reported offsets 2..8 here.
        assert_eq!(s.prev_grapheme_utf16_offset(5), Some(1));
        assert_eq!(s.next_grapheme_utf16_offset(5), Some(9));
        // Past the end of the string.
        assert_eq!(s.next_grapheme_utf16_offset(usize::MAX), None);
        assert_eq!(s.prev_grapheme_utf16_offset(usize::MAX), Some(10));
        assert_eq!("".next_grapheme_utf16_offset(0), None);
        assert_eq!("".prev_grapheme_utf16_offset(0), None);
    }

    #[test]
    fn grapheme_clamps_round_to_a_cluster_edge() {
        let s = format!("a{SKIN_TONE}b");
        for offset in 0..=1 {
            assert_eq!(s.floor_grapheme_utf16_offset(offset), offset);
            assert_eq!(s.ceil_grapheme_utf16_offset(offset), offset);
        }
        // Inside the skin-tone sequence: down to 1, up to 5.
        for offset in 2..=4 {
            assert_eq!(s.floor_grapheme_utf16_offset(offset), 1);
            assert_eq!(s.ceil_grapheme_utf16_offset(offset), 5);
        }
        assert_eq!(s.floor_grapheme_utf16_offset(5), 5);
        assert_eq!(s.ceil_grapheme_utf16_offset(5), 5);
        assert_eq!(s.floor_grapheme_utf16_offset(usize::MAX), 6);
        assert_eq!(s.ceil_grapheme_utf16_offset(usize::MAX), 6);
        assert_eq!("".floor_grapheme_utf16_offset(7), 0);
        assert_eq!("".ceil_grapheme_utf16_offset(7), 0);
    }

    #[test]
    fn word_steps_land_on_cluster_boundaries() {
        let s = "hello world";
        assert_eq!(s.next_word_end_utf16(0), 5);
        assert_eq!(s.next_word_end_utf16(3), 5);
        assert_eq!(s.next_word_end_utf16(5), 11);
        assert_eq!(s.next_word_end_utf16(11), 11);
        assert_eq!(s.next_word_end_utf16(usize::MAX), 11);
        assert_eq!(s.prev_word_start_utf16(11), 6);
        assert_eq!(s.prev_word_start_utf16(7), 6);
        assert_eq!(s.prev_word_start_utf16(6), 0);
        assert_eq!(s.prev_word_start_utf16(0), 0);

        // Punctuation is a separator, not part of a word — UAX #29's own
        // word boundaries would make "foo.bar" one word and change what
        // Ctrl+Left does in ASCII text.
        assert_eq!("foo.bar".next_word_end_utf16(0), 3);
        assert_eq!("foo.bar".next_word_end_utf16(3), 7);
        assert_eq!("foo.bar".next_word_end_utf16(7), 7);
        assert_eq!("foo.bar".prev_word_start_utf16(7), 4);
        assert_eq!("foo.bar".prev_word_start_utf16(4), 0);

        // Text with no words at all steps to its edges.
        assert_eq!(FLAG.next_word_end_utf16(0), 4);
        assert_eq!(FLAG.prev_word_start_utf16(4), 0);

        // A combining mark is not alphanumeric, so scanning `char`s for the
        // first non-alphanumeric one used to stop at offset 1, inside the
        // cluster. A word boundary cannot.
        let s = format!("{ACUTE_E}f");
        assert_eq!(s.next_word_end_utf16(0), 3);
        assert_eq!(s.prev_word_start_utf16(3), 0);

        // Emoji are not words, so a word step moves past them to the text.
        let s = format!("{FAMILY} ab");
        assert_eq!(s.next_word_end_utf16(0), 11);
        assert_eq!(s.prev_word_start_utf16(11), 9);
    }

    #[test]
    fn line_breaks_are_whole_clusters() {
        let ranges = |s: &str| -> Vec<Range<usize>> { s.utf16_line_break_ranges().collect() };
        assert_eq!(ranges("ab\ncd"), vec![2..3]);
        // CRLF is one break, so no offset falls between the two halves.
        assert_eq!(ranges("ab\r\ncd"), vec![2..4]);
        assert_eq!(ranges("a\rb\nc"), vec![1..2, 3..4]);
        assert_eq!(ranges("no breaks"), vec![]);
        // An astral char before the break shifts it by two code units.
        assert_eq!(ranges(&format!("{ASTRAL}\nx")), vec![2..3]);
        assert_eq!(ranges("a\u{2028}b"), vec![1..2]);
    }

    #[test]
    fn remove_utf16_removes_whole_characters() {
        let mut s = String::from("hello world");
        assert!(!s.remove_utf16(0..0));
        assert_eq!(s, "hello world");
        assert!(s.remove_utf16(5..6));
        assert_eq!(s, "helloworld");
        assert!(s.remove_utf16(4..7));
        assert_eq!(s, "hellrld");
        assert!(s.remove_utf16(6..7));
        assert_eq!(s, "hellrl");
        assert!(!s.remove_utf16(6..6));
        assert_eq!(s, "hellrl");

        let mut s = format!("{ASTRAL}abc");
        assert!(s.remove_utf16(2..5));
        assert_eq!(s, ASTRAL);
    }

    #[test]
    fn remove_utf16_refuses_invalid_ranges() {
        // The panic this replaces: a selection left over from longer text.
        let mut s = String::from("abc");
        assert!(!s.remove_utf16(5..7));
        assert_eq!(s, "abc");
        assert!(!s.remove_utf16(0..99));
        assert_eq!(s, "abc");
        assert!(!s.remove_utf16(Range { start: 2, end: 1 }));
        assert_eq!(s, "abc");

        // Half of a surrogate pair is not a removable range.
        let mut s = format!("{ASTRAL}abc");
        assert!(!s.remove_utf16(1..3));
        assert_eq!(s, format!("{ASTRAL}abc"));
    }
}
