//! The editing state of one text field: Flutter's `TextEditingValue`.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::textinput::utils::{OwnedStringUtils, StringUtils};

/// The text and selection of one text field, as the framework sends it over
/// `flutter/textinput`.
///
/// Every offset here — the selection and the composing range — is a UTF-16
/// code-unit offset into `text`, because that is the unit Dart strings are
/// counted in. The editing methods take and return offsets in the same space
/// and never trust them: the framework sends `-1` for "no selection", and the
/// offsets it sends can lag behind the text they arrive with.
///
/// Within that space, every *selection* offset this type works with is pinned
/// to an extended grapheme-cluster boundary of `text`, and every step the
/// caret takes is one cluster or one word. A cluster is what a user sees as a
/// single character and expects one Backspace to remove: "e" followed by
/// U+0301, a flag, a skin-toned hand, a ZWJ family. Stepping by code unit, or
/// even by `char`, takes those apart. The composing range is deliberately *not*
/// pinned that way — it is a span of code units an IME owns, and may well cut
/// a cluster in half while a composition is in progress.
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TextEditingState {
    composing_base: i64,
    composing_extent: i64,
    selection_affinity: String,
    selection_base: i64,
    selection_extent: i64,
    selection_is_directional: bool,
    text: String,
}

/// A UTF-16 offset as the framework wants it back.
fn to_dart_offset(offset: usize) -> i64 {
    // Only reachable for a string longer than `i64::MAX` UTF-16 units, which
    // cannot be allocated on any target this runs on.
    i64::try_from(offset).unwrap_or(i64::MAX)
}

/// Clamp a composing-range offset, keeping the `-1` "not composing" sentinel.
///
/// Clamped to a character boundary rather than a cluster boundary: the
/// composing range belongs to the IME, which may legitimately be composing
/// half of a cluster, and it is never used as a position to edit at.
fn clamp_composing_offset(text: &str, offset: i64) -> i64 {
    if offset < 0 {
        // `TextRange.empty` is `(-1, -1)`; anything else would tell the
        // framework that an IME composition is in progress.
        return -1;
    }
    to_dart_offset(text.clamp_utf16_offset(usize::try_from(offset).unwrap_or(0)))
}

impl TextEditingState {
    /// Pin the offsets Dart sent to valid offsets into [`text`](Self::text).
    ///
    /// The framework asserts its own invariant on the way out but whitelists
    /// the `-1` sentinels, and a `setEditingState` can carry offsets that
    /// belong to text it has already replaced. Clamping once, here at the
    /// boundary, keeps the rest of the model from having to reason about the
    /// whole `i64` range — it still clamps defensively, because
    /// [`TextInputPlugin::with_state`](crate::textinput::TextInputPlugin::with_state)
    /// hands this state out for embedders to drive directly.
    pub(crate) fn normalize(&mut self) {
        self.selection_base = to_dart_offset(self.clamp_offset(self.selection_base));
        self.selection_extent = to_dart_offset(self.clamp_offset(self.selection_extent));
        self.composing_base = clamp_composing_offset(&self.text, self.composing_base);
        self.composing_extent = clamp_composing_offset(&self.text, self.composing_extent);
    }

    /// The text being edited.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Clamp a Dart-supplied offset to a grapheme-cluster boundary in `text`.
    ///
    /// Negative offsets — including the `-1` the framework sends for "no
    /// selection" — resolve to the start of the text, offsets past its end to
    /// its end, and an offset inside a cluster (inside a surrogate pair, or
    /// between an "e" and its combining accent) to the boundary before it,
    /// because an edit at such an offset would split the cluster.
    fn clamp_offset(&self, offset: i64) -> usize {
        self.text
            .floor_grapheme_utf16_offset(usize::try_from(offset).unwrap_or(0))
    }

    /// The selection as an ordered range of clamped UTF-16 offsets.
    fn get_selection_range(&self) -> Range<usize> {
        let base = self.clamp_offset(self.selection_base);
        let extent = self.clamp_offset(self.selection_extent);
        if base <= extent {
            base..extent
        } else {
            extent..base
        }
    }

    /// Collapse the selection to a caret at the UTF-16 offset `p`.
    ///
    /// `p` is clamped into the text and onto a grapheme-cluster boundary, so
    /// that what this state reports back to the framework is always a
    /// selection the framework considers valid, and always a position the
    /// next edit can run at.
    pub fn move_to(&mut self, p: usize) {
        self.selection_base = to_dart_offset(self.text.floor_grapheme_utf16_offset(p));
        self.selection_extent = self.selection_base;
        self.selection_is_directional = false;
    }

    /// Extend the selection to the UTF-16 offset `p`, keeping its base.
    ///
    /// `p` is clamped into the text, as in [`move_to`](Self::move_to).
    pub fn select_to(&mut self, p: usize) {
        self.selection_extent = to_dart_offset(self.text.floor_grapheme_utf16_offset(p));
        self.selection_is_directional = true;
    }

    fn select_or_move_to(&mut self, p: usize, select: bool) {
        if select {
            self.select_to(p);
        } else {
            self.move_to(p);
        }
    }

    /// Select the whole text.
    pub fn select_all(&mut self) {
        self.selection_base = 0;
        self.move_to_end(true);
    }

    /// Delete the selected text, if any.
    ///
    /// Returns `true` if the text changed. A selection that has drifted out
    /// of step with the text is a no-op rather than an error.
    pub fn delete_selected(&mut self) -> bool {
        let range = self.get_selection_range();
        let start = range.start;
        if self.text.remove_utf16(range) {
            self.move_to(start);
            true
        } else {
            false
        }
    }

    /// Replace the selection with `c`, or insert it at the caret.
    pub fn add_characters(&mut self, c: &str) {
        self.delete_selected();
        let pos = self.clamp_offset(self.selection_extent);
        let index = self
            .text
            .byte_offset_of_utf16(pos)
            .unwrap_or(self.text.len());
        self.text.insert_str(index, c);
        // Rounded *up*: if what was typed merged into the cluster that
        // follows — a combining mark already sitting after the caret — the
        // caret belongs after that cluster, not back in front of the
        // insertion.
        let caret = self.text.ceil_grapheme_utf16_offset(pos + c.utf16_len());
        self.move_to(caret);
    }

    /// Delete the selection, or the grapheme cluster before the caret.
    pub fn backspace(&mut self) {
        if self.delete_selected() {
            return;
        }
        let pos = self.clamp_offset(self.selection_base);
        let Some(prev) = self.text.prev_grapheme_utf16_offset(pos) else {
            // Already at the start of the text.
            return;
        };
        if self.text.remove_utf16(prev..pos) {
            self.move_to(prev);
        }
    }

    /// Delete the selection, or the grapheme cluster after the caret.
    pub fn delete(&mut self) {
        if self.delete_selected() {
            return;
        }
        let pos = self.clamp_offset(self.selection_base);
        let Some(next) = self.text.next_grapheme_utf16_offset(pos) else {
            // Already at the end of the text.
            return;
        };
        self.text.remove_utf16(pos..next);
    }

    /// Move the caret one grapheme cluster, or one word, to the left.
    ///
    /// With `select`, the selection's extent moves and its base stays put.
    pub fn move_left(&mut self, by_word: bool, select: bool) {
        let selection = self.get_selection_range();
        let caret = if select {
            self.clamp_offset(self.selection_extent)
        } else {
            selection.start
        };
        // Moving rather than extending with something selected collapses the
        // caret onto the selection's left edge instead of stepping past it.
        let collapsing = !select && selection.start != selection.end;
        let next_pos = if by_word {
            // The word step looks strictly to the left of where it starts;
            // from a collapsed edge that step is the collapse itself, so it
            // runs from one cluster further right and leaves an edge that is
            // already a word start where it is.
            let from = if collapsing {
                self.text.next_grapheme_utf16_offset(caret).unwrap_or(caret)
            } else {
                caret
            };
            self.text.prev_word_start_utf16(from)
        } else if collapsing {
            caret
        } else {
            self.text.prev_grapheme_utf16_offset(caret).unwrap_or(0)
        };
        self.select_or_move_to(next_pos, select);
    }

    /// Move the caret one grapheme cluster, or one word, to the right.
    ///
    /// With `select`, the selection's extent moves and its base stays put.
    pub fn move_right(&mut self, by_word: bool, select: bool) {
        let selection = self.get_selection_range();
        let caret = if select {
            self.clamp_offset(self.selection_extent)
        } else {
            selection.end
        };
        // Mirror of `move_left`: collapse onto the selection's right edge.
        let collapsing = !select && selection.start != selection.end;
        let next_pos = if by_word {
            let from = if collapsing {
                self.text.prev_grapheme_utf16_offset(caret).unwrap_or(caret)
            } else {
                caret
            };
            self.text.next_word_end_utf16(from)
        } else if collapsing {
            caret
        } else {
            self.text
                .next_grapheme_utf16_offset(caret)
                .unwrap_or_else(|| self.text.utf16_len())
        };
        self.select_or_move_to(next_pos, select);
    }

    /// Move the caret to the start of the text.
    pub fn move_to_beginning(&mut self, select: bool) {
        self.select_or_move_to(0, select);
    }

    /// Move the caret to the end of the text.
    pub fn move_to_end(&mut self, select: bool) {
        self.select_or_move_to(self.text.utf16_len(), select);
    }

    /// Move the caret to the line above.
    ///
    /// Approximated as the start of the current line, because real vertical
    /// movement needs the font metrics the embedder does not have. The offset
    /// it lands on is always a grapheme-cluster boundary.
    pub fn move_up(&mut self, select: bool) {
        let selection = self.get_selection_range();

        let p = self.get_next_line_pos(selection.start, false);
        self.select_or_move_to(p, select);
    }

    /// Move the caret to the line below.
    ///
    /// Approximated as the end of the current line, the mirror of
    /// [`move_up`](Self::move_up).
    pub fn move_down(&mut self, select: bool) {
        let selection = self.get_selection_range();

        let p = self.get_next_line_pos(selection.end, true);
        self.select_or_move_to(p, select);
    }

    /// The selected text, or `""` when nothing is selected.
    #[must_use]
    pub fn get_selected_text(&self) -> &str {
        self.text
            .byte_range_of_utf16(self.get_selection_range())
            .map_or("", |range| &self.text[range])
    }

    /// Naive implementation, since rust does not know font metrics.
    /// It's hard to predict column position when caret jumps across lines.
    /// Official android implementation does not have a solution so far:
    /// <https://github.com/flutter/engine/blob/395937380c26c7f7e3e0d781d111667daad2c47d/shell/platform/android/io/flutter/plugin/editing/InputConnectionAdaptor.java>
    ///
    /// So this walks to the end (`forward`) or the start of the current line
    /// instead of keeping a column. The approximation stays, but the offsets
    /// it returns are cluster boundaries: a line break is matched as a whole
    /// cluster, so a `"\r\n"` is one break and the caret never lands between
    /// its halves.
    fn get_next_line_pos(&self, start: usize, forward: bool) -> usize {
        let max = self.text.utf16_len();
        let start = self.text.floor_grapheme_utf16_offset(start);
        if forward {
            // The start of the next line break after `start`, i.e. the end of
            // this line.
            if start >= max {
                return max;
            }
            self.text
                .utf16_line_break_ranges()
                .find(|brk| brk.start > start)
                .map_or(max, |brk| brk.start)
        } else {
            // One past the last line break before `start`, i.e. the start of
            // this line. The cluster immediately before `start` is skipped, so
            // that a caret just after a break walks past it.
            let Some(limit) = self.text.prev_grapheme_utf16_offset(start) else {
                return 0;
            };
            self.text
                .utf16_line_break_ranges()
                .take_while(|brk| brk.start < limit)
                .last()
                .map_or(0, |brk| brk.end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thumbs up, U+1F44D: one char, two UTF-16 code units.
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

    fn state(text: &str, base: i64, extent: i64) -> TextEditingState {
        TextEditingState {
            selection_base: base,
            selection_extent: extent,
            text: text.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn no_selection_sentinel_does_not_panic() {
        // The framework sends `TextSelection.collapsed(offset: -1)` for a
        // field that has never been selected into. `-1 as usize` used to make
        // every one of these overflow or index out of bounds.
        for text in ["", "abc", "a\nb", "👍abc", "שלום", ACUTE_E, FAMILY, FLAG] {
            let mut s = state(text, -1, -1);
            s.move_up(false);
            s.move_down(false);
            s.move_up(true);
            s.move_down(true);
            s.move_left(false, false);
            s.move_right(false, false);
            s.move_left(true, false);
            s.move_right(true, false);
            s.move_left(true, true);
            s.move_right(true, true);
            s.move_to_beginning(false);
            s.move_to_end(false);
            s.select_all();
            assert_eq!(s.get_selected_text(), text);

            let mut s = state(text, -1, -1);
            s.backspace();
            assert_eq!(s.text, text, "backspace at offset 0 is a no-op");
            s.add_characters("x");
            assert_eq!(s.text, format!("x{text}"));

            // Forward delete from the resolved caret at 0 eats the first
            // cluster, which is what a caret at 0 should do.
            let mut s = state(text, -1, -1);
            s.delete();
            assert!(!s.delete_selected());
        }
    }

    #[test]
    fn stale_out_of_range_selection_does_not_panic() {
        let mut s = state("abc", 5, 7);
        assert!(!s.delete_selected());
        assert_eq!(s.text, "abc");
        assert_eq!(s.get_selected_text(), "");
        s.backspace();
        assert_eq!(s.text, "ab");
    }

    #[test]
    fn word_boundary_does_not_underflow() {
        // `len - start` used to underflow for any offset past the text.
        let mut s = state("abc", 99, 99);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 0);
        let mut s = state("abc", 99, 99);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 3);
    }

    #[test]
    fn move_up_does_not_slice_out_of_bounds() {
        // `&v[..start - 1]` used to panic in release builds too.
        let mut s = state("ab\ncd", 99, 99);
        s.move_up(false);
        assert_eq!(s.selection_base, 3);
    }

    #[test]
    fn delete_selected_uses_utf16_offsets() {
        let mut s = state("👍abc", 2, 5); // Dart's offsets for "abc"
        assert!(s.delete_selected());
        assert_eq!(s.text, "👍");
        assert_eq!(s.selection_base, 2);
    }

    #[test]
    fn get_selected_text_uses_utf16_offsets() {
        let s = state("👍abc", 0, 2);
        assert_eq!(s.get_selected_text(), ASTRAL);
        let s = state("👍abc", 2, 3);
        assert_eq!(s.get_selected_text(), "a");
    }

    #[test]
    fn reversed_selection_is_ordered() {
        let mut s = state("hello world", 11, 6);
        assert_eq!(s.get_selected_text(), "world");
        assert!(s.delete_selected());
        assert_eq!(s.text, "hello ");
        assert_eq!(s.selection_base, 6);
    }

    #[test]
    fn reversed_selection_over_a_cluster_is_ordered() {
        // Extent before base, and both ends on cluster boundaries of a text
        // whose clusters are wider than one code unit.
        let text = format!("a{FAMILY}{FLAG}b");
        let mut s = state(&text, 13, 1);
        assert_eq!(s.get_selected_text(), format!("{FAMILY}{FLAG}"));
        assert!(s.delete_selected());
        assert_eq!(s.text, "ab");
        assert_eq!((s.selection_base, s.selection_extent), (1, 1));
    }

    #[test]
    fn backspace_removes_astral_char_at_end() {
        let mut s = state("👍👍👍", 6, 6);
        s.backspace();
        assert_eq!(s.text, "👍👍");
        assert_eq!(s.selection_base, 4);
    }

    #[test]
    fn delete_removes_astral_char_at_caret() {
        let mut s = state("a👍b", 1, 1);
        s.delete();
        assert_eq!(s.text, "ab");
        assert_eq!(s.selection_base, 1);
    }

    #[test]
    fn move_to_end_reports_utf16_offset() {
        let mut s = state("a👍b", 0, 0);
        s.move_to_end(false);
        assert_eq!(s.selection_base, 4);
    }

    #[test]
    fn arrow_keys_step_over_an_astral_char() {
        let mut s = state("a👍b", 1, 1);
        s.move_right(false, false);
        assert_eq!(s.selection_base, 3);
        s.move_right(false, false);
        assert_eq!(s.selection_base, 4);
        s.move_right(false, false);
        assert_eq!(s.selection_base, 4);
        s.move_left(false, false);
        assert_eq!(s.selection_base, 3);
        s.move_left(false, false);
        assert_eq!(s.selection_base, 1);
        s.move_left(false, false);
        assert_eq!(s.selection_base, 0);
        s.move_left(false, false);
        assert_eq!(s.selection_base, 0);
    }

    #[test]
    fn offset_inside_a_surrogate_pair_snaps_to_a_boundary() {
        // Only a buggy framework sends this, but it must not corrupt `text`.
        let mut s = state("👍abc", 1, 1);
        s.add_characters("x");
        assert_eq!(s.text, "x👍abc");
        assert_eq!(s.selection_base, 1);
    }

    #[test]
    fn backspace_removes_one_whole_cluster() {
        // Every one of these used to take two or more Backspaces, and left
        // the cluster in pieces in between.
        for cluster in [ACUTE_E, SKIN_TONE, FAMILY, FLAG] {
            let text = format!("a{cluster}");
            let mut s = state(&text, to_dart_offset(text.utf16_len()), 0);
            s.move_to_end(false);
            s.backspace();
            assert_eq!(s.text, "a", "backspacing {cluster:?} left pieces behind");
            assert_eq!(s.selection_base, 1);
            s.backspace();
            assert_eq!(s.text, "");
        }
    }

    #[test]
    fn delete_removes_one_whole_cluster() {
        for cluster in [ACUTE_E, SKIN_TONE, FAMILY, FLAG] {
            let mut s = state(&format!("{cluster}z"), 0, 0);
            s.delete();
            assert_eq!(s.text, "z", "deleting {cluster:?} left pieces behind");
            assert_eq!(s.selection_base, 0);
        }
    }

    #[test]
    fn arrow_keys_step_over_one_whole_cluster() {
        for cluster in [ACUTE_E, SKIN_TONE, FAMILY, FLAG] {
            let width = to_dart_offset(cluster.utf16_len());
            let text = format!("a{cluster}b");
            let mut s = state(&text, 1, 1);
            s.move_right(false, false);
            assert_eq!(s.selection_base, 1 + width, "right over {cluster:?}");
            s.move_left(false, false);
            assert_eq!(s.selection_base, 1, "left over {cluster:?}");
        }
    }

    #[test]
    fn extending_the_selection_steps_by_cluster() {
        let text = format!("a{FAMILY}b");
        let mut s = state(&text, 1, 1);
        s.move_right(false, true);
        assert_eq!((s.selection_base, s.selection_extent), (1, 9));
        assert_eq!(s.get_selected_text(), FAMILY);
        s.move_right(false, true);
        assert_eq!((s.selection_base, s.selection_extent), (1, 10));
        s.move_left(false, true);
        assert_eq!((s.selection_base, s.selection_extent), (1, 9));
        s.move_left(false, true);
        assert_eq!((s.selection_base, s.selection_extent), (1, 1));
        assert_eq!(s.get_selected_text(), "");
    }

    #[test]
    fn two_flags_are_two_clusters() {
        // Regional indicators pair up, so a run of four of them is two flags
        // and takes two Backspaces — not one, and not four.
        let text = format!("{FLAG}{FLAG}");
        let mut s = state(&text, 8, 8);
        s.backspace();
        assert_eq!(s.text, FLAG);
        assert_eq!(s.selection_base, 4);
        s.backspace();
        assert_eq!(s.text, "");
    }

    #[test]
    fn combining_marks_are_edited_as_one_grapheme() {
        // "e" + U+0301 COMBINING ACUTE ACCENT: two chars, two UTF-16 units,
        // one grapheme cluster. The second backspace used to leave a bare
        // "e" behind, having removed only the accent.
        let mut s = state("e\u{301}f", 3, 3);
        s.backspace();
        assert_eq!(s.text, "e\u{301}");
        s.backspace();
        assert_eq!(s.text, "");
        assert_eq!(s.selection_base, 0);
    }

    #[test]
    fn typing_a_combining_mark_keeps_the_caret_after_it() {
        // A dead key delivers the accent on its own, after the letter.
        let mut s = state("e", 1, 1);
        s.add_characters("\u{301}");
        assert_eq!(s.text, "e\u{301}");
        assert_eq!(s.selection_base, 2);

        // The other order: the accent is already there and the letter is
        // typed in front of it, merging into its cluster. Flooring the caret
        // would park it before the letter just typed.
        let mut s = state("\u{301}x", 0, 0);
        s.add_characters("e");
        assert_eq!(s.text, "e\u{301}x");
        assert_eq!(s.selection_base, 2);
    }

    #[test]
    fn rtl_text_is_edited_in_logical_order() {
        // Hebrew is BMP, so UTF-16 offsets are char offsets here; the point
        // is that the model works in logical, not visual, order.
        let mut s = state("שלום", 0, 2);
        assert_eq!(s.get_selected_text(), "של");
        assert!(s.delete_selected());
        assert_eq!(s.text, "ום");
        s.move_to_end(false);
        assert_eq!(s.selection_base, 2);
    }

    #[test]
    fn rtl_text_with_marks_steps_by_cluster() {
        // Hebrew letter + niqqud: the vowel point is a combining mark, so
        // "בָּ" is one cluster of three code units.
        let text = "בָּא";
        assert_eq!(text.utf16_len(), 4);
        let mut s = state(text, 0, 0);
        s.move_right(false, false);
        assert_eq!(s.selection_base, 3);
        s.backspace();
        assert_eq!(s.text, "א");
        s.move_to_end(false);
        s.backspace();
        assert_eq!(s.text, "");
    }

    #[test]
    fn ascii_editing_round_trip_is_unchanged() {
        let mut s = state("hello world", 0, 0);
        s.move_to_end(false);
        assert_eq!(s.selection_base, 11);
        s.backspace();
        assert_eq!(s.text, "hello worl");
        s.move_to(5);
        s.delete();
        assert_eq!(s.text, "helloworl");
        s.add_characters(" ");
        assert_eq!(s.text, "hello worl");
        assert_eq!(s.selection_base, 6);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 0);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 5);
        s.select_all();
        assert_eq!(s.get_selected_text(), "hello worl");
        assert!(s.delete_selected());
        assert_eq!(s.text, "");
    }

    #[test]
    fn ascii_word_movement_is_unchanged() {
        let mut s = state("foo.bar baz", 0, 0);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 3);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 7);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 11);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 11);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 8);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 4);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 0);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 0);
    }

    #[test]
    fn word_movement_does_not_split_a_cluster() {
        // A combining mark is not alphanumeric, so the old scan for the first
        // non-alphanumeric `char` stopped at offset 1, between the "e" and
        // its accent — a caret position that cannot be edited at.
        let mut s = state("e\u{301}f g", 0, 0);
        s.move_right(true, false);
        assert_eq!(s.selection_base, 3);
        s.move_to(5);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 4);
        s.move_left(true, false);
        assert_eq!(s.selection_base, 0);

        // Emoji are not words, so a word step moves across the whole run of
        // them rather than into one.
        let text = format!("{FAMILY}{SKIN_TONE} ab");
        let mut s = state(&text, 0, 0);
        s.move_right(true, false);
        assert_eq!(s.selection_base, to_dart_offset(text.utf16_len()));
        s.move_left(true, false);
        assert_eq!(s.selection_base, 13);
    }

    #[test]
    fn moving_with_a_selection_collapses_the_caret() {
        let mut s = state("hello world", 2, 5);
        s.move_left(false, false);
        assert_eq!((s.selection_base, s.selection_extent), (2, 2));
        let mut s = state("hello world", 2, 5);
        s.move_right(false, false);
        assert_eq!((s.selection_base, s.selection_extent), (5, 5));
        // Extending keeps the base where it was.
        let mut s = state("hello world", 2, 5);
        s.move_right(false, true);
        assert_eq!((s.selection_base, s.selection_extent), (2, 6));
    }

    #[test]
    fn collapsing_onto_a_cluster_edge_does_not_split_it() {
        // Selection over the family only; the collapse has to land on 1 or 9,
        // never between the ZWJ-joined pieces.
        let text = format!("a{FAMILY}b");
        let mut s = state(&text, 1, 9);
        s.move_left(false, false);
        assert_eq!((s.selection_base, s.selection_extent), (1, 1));
        let mut s = state(&text, 1, 9);
        s.move_right(false, false);
        assert_eq!((s.selection_base, s.selection_extent), (9, 9));
        // Reversed, and with the word variants, which take their own step.
        let mut s = state(&text, 9, 1);
        s.move_left(true, false);
        assert_eq!((s.selection_base, s.selection_extent), (0, 0));
        let mut s = state(&text, 9, 1);
        s.move_right(true, false);
        assert_eq!((s.selection_base, s.selection_extent), (10, 10));
    }

    #[test]
    fn word_movement_with_a_selection_collapses_onto_its_edge() {
        // The collapse is the word step's own step, so an edge that already
        // sits on a word start stays there.
        let mut s = state("hello world", 6, 9);
        s.move_left(true, false);
        assert_eq!((s.selection_base, s.selection_extent), (6, 6));
        let mut s = state("hello world", 2, 5);
        s.move_right(true, false);
        assert_eq!((s.selection_base, s.selection_extent), (5, 5));
    }

    #[test]
    fn line_movement_walks_multiline_text() {
        // The implementation is the naive one described on
        // `get_next_line_pos`: it finds the bounds of the current line.
        let mut s = state("ab\ncd", 4, 4);
        s.move_up(false);
        assert_eq!(s.selection_base, 3);
        let mut s = state("ab\ncd", 1, 1);
        s.move_down(false);
        assert_eq!(s.selection_base, 2);
        let mut s = state("ab\ncd", 5, 5);
        s.move_down(false);
        assert_eq!(s.selection_base, 5);
    }

    #[test]
    fn line_movement_treats_crlf_as_one_break() {
        // The old scan matched the '\n' of a "\r\n" and reported the offset
        // between the two halves of that cluster.
        let mut s = state("ab\r\ncd", 6, 6);
        s.move_up(false);
        assert_eq!(s.selection_base, 4);
        let mut s = state("ab\r\ncd", 1, 1);
        s.move_down(false);
        assert_eq!(s.selection_base, 2);
        // Deleting from the start of the second line removes the whole break.
        let mut s = state("ab\r\ncd", 4, 4);
        s.backspace();
        assert_eq!(s.text, "abcd");
        assert_eq!(s.selection_base, 2);
    }

    #[test]
    fn line_movement_does_not_split_a_cluster() {
        let text = format!("{FAMILY}\n{FLAG}");
        let mut s = state(&text, 13, 13);
        s.move_up(false);
        assert_eq!(s.selection_base, 9);
        let mut s = state(&text, 0, 0);
        s.move_down(false);
        assert_eq!(s.selection_base, 8);
    }

    #[test]
    fn move_to_clamps_what_an_embedder_asks_for() {
        // `with_state` hands this type to embedders, and an offset that is
        // not in the text would fail the framework's own assertion once
        // `notify_changes` reported it back.
        let mut s = state("a👍b", 0, 0);
        s.move_to(99);
        assert_eq!((s.selection_base, s.selection_extent), (4, 4));
        s.select_to(2); // inside the surrogate pair
        assert_eq!(s.selection_extent, 1);

        // And inside a cluster made of whole characters.
        let text = format!("a{FAMILY}b");
        let mut s = state(&text, 0, 0);
        s.move_to(5);
        assert_eq!((s.selection_base, s.selection_extent), (1, 1));
        s.select_to(7);
        assert_eq!(s.selection_extent, 1);
    }

    #[test]
    fn normalize_clamps_the_offsets_dart_sent() {
        let mut s = state("abc", -1, 99);
        s.composing_base = -1;
        s.composing_extent = 99;
        s.normalize();
        assert_eq!((s.selection_base, s.selection_extent), (0, 3));
        // `-1` means "not composing" and has to survive.
        assert_eq!((s.composing_base, s.composing_extent), (-1, 3));

        let mut s = state("👍abc", 1, 6);
        s.normalize();
        assert_eq!((s.selection_base, s.selection_extent), (0, 5));
    }

    #[test]
    fn normalize_pins_the_selection_but_not_the_composing_range() {
        // A caret inside a cluster is not an editing position, so it is
        // pinned to the cluster's start. The composing range is a span of
        // code units the IME owns, so it only has to be a character boundary.
        let mut s = state(&format!("{ACUTE_E}x"), 1, 1);
        s.composing_base = 0;
        s.composing_extent = 1;
        s.normalize();
        assert_eq!((s.selection_base, s.selection_extent), (0, 0));
        assert_eq!((s.composing_base, s.composing_extent), (0, 1));
    }
}
