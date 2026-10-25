//! Offset arithmetic between Rust byte offsets and the UTF-16 code-unit
//! offsets Flutter's text input protocol speaks.
//!
//! Dart strings are UTF-16, so every offset in a `TextEditingValue` — the
//! selection, the composing range — counts UTF-16 code units, while Rust
//! strings are UTF-8 and index in bytes. A character outside the Basic
//! Multilingual Plane (an emoji, say) is one `char`, two UTF-16 code units and
//! four UTF-8 bytes, so the two offset spaces only agree on text that stays
//! inside the BMP. Everything here converts Dart's offsets into ours, and
//! reports an offset that does not land on a character boundary instead of
//! panicking or silently misplacing the caret.

use std::ops::Range;

/// UTF-16 offset arithmetic on string slices.
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

    /// The UTF-16 offset of the character boundary before `u16_offset`.
    ///
    /// Returns `None` at the start of the string, and for an offset that is
    /// not a character boundary.
    fn prev_utf16_offset(&self, u16_offset: usize) -> Option<usize>;

    /// The UTF-16 offset of the character boundary after `u16_offset`.
    ///
    /// Returns `None` at the end of the string, and for an offset that is not
    /// a character boundary.
    fn next_utf16_offset(&self, u16_offset: usize) -> Option<usize>;

    /// `u16_offset` pinned to the character boundary at or before it, and to
    /// the end of the string when it points past it.
    ///
    /// This is how an offset from Dart becomes one of ours: the framework
    /// sends offsets that can lag behind the text they arrive with, and a
    /// caret inside a surrogate pair is not a position we can edit at.
    fn clamp_utf16_offset(&self, u16_offset: usize) -> usize;

    /// `(utf16 offset, char)` for every character of this string, in order.
    fn utf16_char_offsets(&self) -> impl Iterator<Item = (usize, char)>;
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

    fn prev_utf16_offset(&self, u16_offset: usize) -> Option<usize> {
        let byte_offset = self.byte_offset_of_utf16(u16_offset)?;
        let c = self[..byte_offset].chars().next_back()?;
        Some(u16_offset - c.len_utf16())
    }

    fn next_utf16_offset(&self, u16_offset: usize) -> Option<usize> {
        let byte_offset = self.byte_offset_of_utf16(u16_offset)?;
        let c = self[byte_offset..].chars().next()?;
        Some(u16_offset + c.len_utf16())
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

    fn utf16_char_offsets(&self) -> impl Iterator<Item = (usize, char)> {
        self.chars().scan(0, |units, c| {
            let offset = *units;
            *units += c.len_utf16();
            Some((offset, c))
        })
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

    #[test]
    fn utf16_len_counts_code_units() {
        assert_eq!("".utf16_len(), 0);
        assert_eq!("hello world".utf16_len(), 11);
        // Three BMP chars, three bytes each.
        assert_eq!("日本語".utf16_len(), 3);
        assert_eq!(format!("{ASTRAL}abc").utf16_len(), 5);
        assert_eq!("e\u{301}".utf16_len(), 2);
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
    fn neighbouring_boundaries_step_by_character() {
        let s = format!("a{ASTRAL}b");
        assert_eq!(s.next_utf16_offset(0), Some(1));
        assert_eq!(s.next_utf16_offset(1), Some(3));
        assert_eq!(s.next_utf16_offset(3), Some(4));
        assert_eq!(s.next_utf16_offset(4), None);
        assert_eq!(s.prev_utf16_offset(4), Some(3));
        assert_eq!(s.prev_utf16_offset(3), Some(1));
        assert_eq!(s.prev_utf16_offset(1), Some(0));
        assert_eq!(s.prev_utf16_offset(0), None);
        // Inside the surrogate pair.
        assert_eq!(s.next_utf16_offset(2), None);
        assert_eq!(s.prev_utf16_offset(2), None);
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
    }

    #[test]
    fn utf16_char_offsets_counts_along() {
        let s = format!("a{ASTRAL}b");
        let offsets: Vec<_> = s.utf16_char_offsets().collect();
        assert_eq!(offsets, vec![(0, 'a'), (1, '\u{1f44d}'), (3, 'b')]);
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
