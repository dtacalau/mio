//! Primitives for working with UDP.
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

#[cfg(debug_assertions)]
use crate::poll::SelectorId;
use crate::{sys};
//use crate::{event,  Interest, Registry, Token};
use std::fmt;
use std::io;
use std::sync::Arc;
//#[cfg(windows)]
//use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};

use std::path::Path;
use winapi::um::minwinbase::OVERLAPPED;

/// abc
/// abc
pub struct AsyncFile {
    /// abc
    pub sys: Arc<sys::AsyncFile>,
    #[cfg(debug_assertions)]
    selector_id: SelectorId,
}

impl AsyncFile {
    /// abc
    /// abc
    pub fn open(path: &Path) -> io::Result<AsyncFile> {
        sys::AsyncFile::open(path).map(|af| AsyncFile {
            sys: Arc::new(af),
            #[cfg(debug_assertions)]
            selector_id: SelectorId::new(),
        })
    }

    /// abc
    pub unsafe fn write(&self, buf: &[u8], overlapped: *mut OVERLAPPED) -> io::Result<()> {
        self.sys.write(buf, overlapped)
    }

    /// # Notes
    ///
    pub unsafe fn read(&self, buf: &mut [u8], overlapped: *mut OVERLAPPED) -> io::Result<()> {
        self.sys.read(buf, overlapped)
    }
}



impl fmt::Debug for AsyncFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.sys, f)
    }
}

