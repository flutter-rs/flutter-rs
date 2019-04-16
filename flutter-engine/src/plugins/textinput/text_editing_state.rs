use crate::utils::{OwnedStringUtils, StringUtils};

use std::ops::Range;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

enum Direction {
    Left,
    Right,
}

impl TextEditingState {
    pub fn from(v: &Value) -> Option<Self> {
        if let Some(m) = v.as_object() {
            Some(Self {
                composing_base: m
                    .get("composingBase")
                    .unwrap_or(&json!(-1))
                    .as_i64()
                    .unwrap(),
                composing_extent: m
                    .get("composingExtent")
                    .unwrap_or(&json!(-1))
                    .as_i64()
                    .unwrap(),
                selection_affinity: String::from(
                    m.get("selectionAffinity")
                        .unwrap_or(&json!(""))
                        .as_str()
                        .unwrap(),
                ),
                selection_base: m
                    .get("selectionBase")
                    .unwrap_or(&json!(-1))
                    .as_i64()
                    .unwrap(),
                selection_extent: m
                    .get("selectionExtent")
                    .unwrap_or(&json!(-1))
                    .as_i64()
                    .unwrap(),
                selection_is_directional: m
                    .get("selectionIsDirectional")
                    .unwrap_or(&json!(false))
                    .as_bool()
                    .unwrap(),
                text: String::from(m.get("text").unwrap_or(&json!("")).as_str().unwrap()),
                ..Default::default()
            })
        } else {
            None
        }
    }

    fn get_selection_range(&self) -> Range<usize> {
        if self.selection_base <= self.selection_extent {
            self.selection_base as usize..self.selection_extent as usize
        } else {
            self.selection_extent as usize..self.selection_base as usize
        }
    }

    pub fn move_to(&mut self, p: usize) {
        self.selection_base = p as i64;
        self.selection_extent = self.selection_base;
        self.selection_is_directional = false;
    }

    pub fn select_to(&mut self, p: usize) {
        self.selection_extent = p as i64;
        self.selection_is_directional = true;
    }

    fn select_or_move_to(&mut self, p: usize, select: bool) {
        if select {
            self.select_to(p)
        } else {
            self.move_to(p)
        }
    }

    pub fn select_all(&mut self) {
        self.selection_base = 0;
        self.move_to_end(true);
    }

    pub fn delete_selected(&mut self) -> bool {
        let range = self.get_selection_range();
        if range.start != range.end {
            self.move_to(range.start);
            self.text.remove_chars(range);
            true
        } else {
            false
        }
    }

    pub fn add_characters(&mut self, c: &str) {
        self.delete_selected();
        let index = self
            .text
            .byte_index_of_char(self.selection_extent as usize)
            .unwrap_or_else(|| self.text.len());
        self.text.insert_str(index, c);
        self.move_to(self.selection_extent as usize + c.char_count());
    }

    pub fn backspace(&mut self) {
        if !self.delete_selected() && self.selection_base > 0 {
            if let Some(index) = self
                .text
                .byte_index_of_char(self.selection_base as usize - 1)
            {
                self.text.remove(index);
                self.move_to(self.selection_base as usize - 1);
            }
        }
    }

    pub fn delete(&mut self) {
        if !self.delete_selected() && (self.selection_base as usize) < self.text.char_count() {
            if let Some(index) = self.text.byte_index_of_char(self.selection_base as usize) {
                self.text.remove(index);
            }
        }
    }

    pub fn move_left(&mut self, by_word: bool, select: bool) {
        let selection = self.get_selection_range();

        let current_pos = if select {
            self.selection_extent as usize
        } else if self.selection_base != self.selection_extent {
            selection.start + 1
        } else {
            selection.start
        };
        let next_pos = if by_word {
            self.get_next_word_boundary(current_pos, Direction::Left)
        } else {
            (current_pos as i64 - 1).max(0) as usize
        };
        self.select_or_move_to(next_pos, select);
    }

    pub fn move_right(&mut self, by_word: bool, select: bool) {
        let selection = self.get_selection_range();

        let current_pos = if select {
            self.selection_extent as usize
        } else if self.selection_base != self.selection_extent {
            selection.end - 1
        } else {
            selection.end
        };
        let next_pos = if by_word {
            self.get_next_word_boundary(current_pos, Direction::Right)
        } else {
            (current_pos + 1).min(self.text.char_count())
        };
        self.select_or_move_to(next_pos, select);
    }

    pub fn move_to_beginning(&mut self, select: bool) {
        self.select_or_move_to(0, select);
    }

    pub fn move_to_end(&mut self, select: bool) {
        self.select_or_move_to(self.text.char_count(), select);
    }

    pub fn get_selected_text(&self) -> &str {
        if let Some(range) = self.text.byte_range_of_chars(self.get_selection_range()) {
            &self.text[range]
        } else {
            ""
        }
    }

    /// Naive implementation, since rust does not know font metrics.
    /// It's hard to predict column position when caret jumps across lines.
    /// Official android implementation does not have a solution so far:
    /// https://github.com/flutter/engine/blob/395937380c26c7f7e3e0d781d111667daad2c47d/shell/platform/android/io/flutter/plugin/editing/InputConnectionAdaptor.java
    fn get_next_line_pos(&self, start: usize, forward: bool) -> usize {
        let v: Vec<char> = self.text.chars().collect();
        if forward {
            // search forward
            let max = self.text.char_count();
            if start >= max {
                return max;
            }
            let s = &v[start + 1..];
            s.iter().position(|&c| c == '\n').map_or(max, |n| {
                // end of line pos
                start + n + 1
            })
        } else {
            // search backward
            if start < 1 {
                return 0;
            }
            let s = &v[..start - 1];
            let len = s.iter().count();
            s.iter().rposition(|&c| c == '\n').map_or(0, |n| {
                // start of line pos
                start - len + n
            })
        }
    }

    fn get_next_word_boundary(&self, start: usize, direction: Direction) -> usize {
        match direction {
            Direction::Right => {
                let max = self.text.char_count();
                if start >= max {
                    return max;
                }
                let start = start + 1;
                self.text
                    .chars()
                    .skip(start)
                    .position(|c| !c.is_alphanumeric())
                    .map_or(max, |n| start + n)
            }
            Direction::Left => {
                if start == 0 {
                    return 0;
                }
                let len = self.text.char_count();
                let start = start - 1;
                self.text
                    .chars()
                    .rev()
                    .skip(len - start)
                    .position(|c| !c.is_alphanumeric())
                    .map_or(0, |n| start - n)
            }
        }
    }
}
