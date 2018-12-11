use std::ffi::CString;
use std::os::raw::{c_char};
use std::{mem, ptr, slice};

pub struct CStringVec {
    inner: Box<[*mut c_char]>
}

impl CStringVec {
    pub fn new(v: &[&str]) -> CStringVec {
        let mut ptrs: Vec<*mut c_char> = Vec::with_capacity(v.len());
        for &s in v {
            let c = CString::new(s).unwrap();
            ptrs.push(c.into_raw());
        }
        CStringVec {
            inner: ptrs.into_boxed_slice()
        }
    }

    /// Bypass "move out of struct which implements [`Drop`] trait" restriction.
    pub fn into_raw(self) -> *mut *mut c_char {
        unsafe {
            let p = ptr::read(&self.inner);
            mem::forget(self);
            Box::into_raw(p) as *mut *mut c_char
        }
    }

    pub fn from_raw(len: usize, ptr: *mut *mut c_char) -> CStringVec {
        unsafe {
            let data = slice::from_raw_parts_mut(ptr, len as usize);
            let inner = Box::from_raw(data);
            CStringVec {
                inner: inner,
            }
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
    fn substring(&self, start: usize, len: usize) -> &str;
    fn count(&self) -> usize;
}

impl StringUtils for String {
    fn substring(&self, start: usize, end: usize) -> &str {
        if start >= end {
            return "";
        }
        let mut iter = self.char_indices().skip(start);
        let start_idx = match iter.next() {
            Some((i, _)) => i,
            None => 0,
        };
        let mut iter = iter.skip(end - start - 1);
        let end_idx = match iter.next() {
            Some((i, _)) => i,
            None => self.len(),
        };
        &self[start_idx .. end_idx]
    }
    fn count(&self) -> usize {
        self.chars().count()
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
}