use std::ops::Range;

pub trait StringUtils {
    fn substring(&self, start: usize, end: usize) -> &str;
    fn char_count(&self) -> usize;
    fn byte_index_of_char(&self, char_index: usize) -> Option<usize>;
    fn byte_range_of_chars(&self, char_range: Range<usize>) -> Option<Range<usize>>;
}

pub trait OwnedStringUtils {
    fn remove_chars(&mut self, range: Range<usize>);
}

impl StringUtils for str {
    fn substring(&self, start: usize, end: usize) -> &str {
        if start >= end {
            return "";
        }
        let start_idx = self.byte_index_of_char(start).unwrap_or(0);
        let end_idx = self.byte_index_of_char(end).unwrap_or_else(|| self.len());
        &self[start_idx..end_idx]
    }
    fn char_count(&self) -> usize {
        self.chars().count()
    }
    fn byte_index_of_char(&self, char_index: usize) -> Option<usize> {
        match self.char_indices().nth(char_index) {
            Some((i, _)) => Some(i),
            None => None,
        }
    }
    fn byte_range_of_chars(&self, char_range: Range<usize>) -> Option<Range<usize>> {
        let mut indices = self.char_indices();
        match indices.nth(char_range.start) {
            Some((start_idx, _)) => {
                if char_range.end <= char_range.start {
                    Some(start_idx..start_idx)
                } else {
                    match indices.nth(char_range.end - char_range.start - 1) {
                        Some((end_idx, _)) => Some(start_idx..end_idx),
                        None => Some(start_idx..self.len()),
                    }
                }
            }
            None => None,
        }
    }
}

impl OwnedStringUtils for String {
    fn remove_chars(&mut self, range: Range<usize>) {
        if range.start >= range.end {
            return;
        }
        self.drain(self.byte_range_of_chars(range).unwrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substring() {
        let s = String::from("hello world");
        assert_eq!(s.substring(0, 0), "");
        assert_eq!(s.substring(0, 1), "h");
        assert_eq!(s.substring(10, 11), "d");
        assert_eq!(s.substring(0, 11), "hello world");
        assert_eq!(s.substring(11, 11), "");
    }

    #[test]
    fn test_remove_chars() {
        let mut s = String::from("hello world");
        s.remove_chars(0..0);
        assert_eq!(s, "hello world");
        s.remove_chars(5..6);
        assert_eq!(s, "helloworld");
        s.remove_chars(4..7);
        assert_eq!(s, "hellrld");
        s.remove_chars(6..7);
        assert_eq!(s, "hellrl");
        s.remove_chars(6..6);
        assert_eq!(s, "hellrl");
    }

    #[test]
    fn test_byte_range() {
        let s = String::from("hello world");
        assert_eq!(s.byte_range_of_chars(0..0), Some(0..0));
        assert_eq!(s.byte_range_of_chars(0..1), Some(0..1));
        assert_eq!(s.byte_range_of_chars(1..0), Some(1..1));
        assert_eq!(s.byte_range_of_chars(1..4), Some(1..4));
        assert_eq!(s.byte_range_of_chars(14..27), None);
        assert_eq!(s.byte_range_of_chars(10..10), Some(10..10));
        assert_eq!(s.byte_range_of_chars(10..11), Some(10..11));
        assert_eq!(s.byte_range_of_chars(10..12), Some(10..11));
        assert_eq!(s.byte_range_of_chars(11..11), None);
    }
}
