use std::ffi::CString;
use std::ops::Range;
use std::os::raw::c_char;
use std::{mem, ptr, slice};

pub struct CStringVec {
    inner: Box<[*mut c_char]>,
}

impl CStringVec {
    pub fn new<T: AsRef<str>>(v: &[T]) -> CStringVec {
        let mut ptrs: Vec<*mut c_char> = Vec::with_capacity(v.len());
        for s in v {
            let c = CString::new(s.as_ref()).unwrap();
            ptrs.push(c.into_raw());
        }
        CStringVec {
            inner: ptrs.into_boxed_slice(),
        }
    }

    /// Bypass "move out of struct which implements [`Drop`] trait" restriction.
    pub fn into_raw(self) -> *const *const c_char {
        unsafe {
            let p = ptr::read(&self.inner);
            mem::forget(self);
            Box::into_raw(p) as *const *const c_char
        }
    }

    pub fn from_raw(len: usize, ptr: *const *const c_char) -> CStringVec {
        unsafe {
            let data = slice::from_raw_parts_mut(ptr as *mut _, len as usize);
            let inner = Box::from_raw(data);
            CStringVec { inner }
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl Drop for CStringVec {
    fn drop(&mut self) {
        unsafe {
            for &v in self.inner.iter() {
                let _ = CString::from_raw(v);
            }
        }
    }
}

pub trait StringUtils {
    fn substring(&self, start: usize, end: usize) -> &str;
    fn remove_chars(&mut self, range: Range<usize>);
    fn count(&self) -> usize;
    fn byte_index_of_char(&self, char_index: usize) -> Option<usize>;
    fn byte_range_of_chars(&self, char_range: Range<usize>) -> Option<Range<usize>>;
}

impl StringUtils for String {
    fn substring(&self, start: usize, end: usize) -> &str {
        if start >= end {
            return "";
        }
        let start_idx = self.byte_index_of_char(start).unwrap_or(0);
        let end_idx = self.byte_index_of_char(end).unwrap_or_else(|| self.len());
        &self[start_idx..end_idx]
    }
    fn remove_chars(&mut self, range: Range<usize>) {
        if range.start >= range.end {
            return;
        }
        self.drain(self.byte_range_of_chars(range).unwrap());
    }
    fn count(&self) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    #[test]
    fn test_cstring_vec() {
        let v = CStringVec::new(&["hello", "world"]);
        let ptr = v.inner[0] as *const c_char;
        unsafe {
            let s1 = CStr::from_ptr(ptr as *mut c_char);
            assert_eq!(s1.to_str().unwrap(), "hello");
        }
        let ptr = v.inner[1] as *const c_char;
        unsafe {
            let s1 = CStr::from_ptr(ptr as *mut c_char);
            assert_eq!(s1.to_str().unwrap(), "world");
        }

        let len = v.len();
        let ptr = v.into_raw();
        let v = CStringVec::from_raw(len, ptr);
        assert_eq!(v.len(), len);
    }

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
