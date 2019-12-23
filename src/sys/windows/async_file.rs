use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};
use std::io;
use std::path::Path;
use std::fs::File;
use std::ptr::null_mut;
use std::cmp;


use winapi::um::fileapi::{CreateFileA, ReadFile, WriteFile, CREATE_NEW, OPEN_EXISTING};
use winapi::um::winnt::{GENERIC_READ, GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::winbase::FILE_FLAG_OVERLAPPED;
use winapi::um::minwinbase::OVERLAPPED;
use winapi::shared::minwindef::DWORD;
use winapi::shared::winerror::ERROR_IO_PENDING;
use winapi::shared::minwindef::FALSE;

pub use super::HasCompletion;
pub use super::CompletionHandler;
use winapi::um::minwinbase::OVERLAPPED_ENTRY;

use ::std;
use std::ffi::CString;

#[derive(Debug)]
pub struct AsyncFile {
    fd: File,
}

impl AsyncFile {
    pub fn open(path: &Path) -> io::Result<AsyncFile> {

        unsafe {
            let path = CString::new(format!("{}", path.display())).expect("invalid file path");
            let h = CreateFileA(
                path.as_ptr(),//lpFileName: LPCSTR,
                GENERIC_READ | GENERIC_WRITE, // dwDesiredAccess: DWORD,
                FILE_SHARE_READ | FILE_SHARE_WRITE, // dwShareMode: DWORD,
                null_mut(), // lpSecurityAttributes: LPSECURITY_ATTRIBUTES,
                CREATE_NEW | OPEN_EXISTING, // dwCreationDisposition: DWORD,
                FILE_FLAG_OVERLAPPED, // dwFlagsAndAttributes: DWORD,
                null_mut(), // hTemplateFile: HANDLE, 
            );
        

        if h == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(AsyncFile{ fd: File::from_raw_handle(h as RawHandle)})
        }
    }
    }
    
    pub unsafe fn read(&self, buf: &mut [u8], overlapped: *mut OVERLAPPED) -> io::Result<()> {
        let len = cmp::min(buf.len(), <DWORD>::max_value() as usize) as DWORD;
        let res = ReadFile(self.fd.as_raw_handle(), buf.as_mut_ptr() as *mut _, len, null_mut(), overlapped);

        if  FALSE == res {
            let last_error = io::Error::last_os_error();
            if  last_error.raw_os_error().unwrap() != ERROR_IO_PENDING as i32 {
                return Err(last_error);
            }
            else {
                return Ok(());
            }
        }

        // we got here if write return TRUE which should not happen because it is async
        Err(io::Error::new(io::ErrorKind::Other,""))
    }

    pub unsafe fn write(&self, buf: &[u8], overlapped: *mut OVERLAPPED) -> io::Result<()> {
        let len = cmp::min(buf.len(), <DWORD>::max_value() as usize) as DWORD;
        let res = WriteFile(self.fd.as_raw_handle(), buf.as_ptr() as *const _, len, null_mut(), overlapped);

        if  FALSE == res {
            let last_error = io::Error::last_os_error();
            if  last_error.raw_os_error().unwrap() != ERROR_IO_PENDING as i32 {
                return Err(last_error);
            }
            else {
                return Ok(());
            }
        }

        // we got here if write return TRUE which should not happen because it is async
        Err(io::Error::new(io::ErrorKind::Other, ""))
    }
}

pub fn completion_handler(oe: &OVERLAPPED_ENTRY) -> Option<bool>
{
    println!("asyn file completion_handler {}", oe.dwNumberOfBytesTransferred);

    None
}

use std::sync::Arc;

impl super::HasCompletion for AsyncFile {
    fn complete(&self, oe: &OVERLAPPED_ENTRY) -> Option<bool> {
        None
    }

    fn get_raw_handle(&self) -> Arc<RawHandle> {
        Arc::new(self.fd.as_raw_handle())
    }

    fn get_completion_handler(&self) -> CompletionHandler {
        completion_handler
    }
}

impl IntoRawHandle for AsyncFile {
    fn into_raw_handle(self) -> RawHandle {
        self.fd.as_raw_handle()
    }
}

impl AsRawHandle for AsyncFile {
    fn as_raw_handle(&self) -> RawHandle {
        self.fd.as_raw_handle()
    }
}