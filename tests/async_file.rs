#![cfg(all(feature = "os-poll", feature = "udp"))]

use mio::net::AsyncFile;
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;

use mio::event::CompletionSource;
use mio::windows::CompletionSourceHandle;

use winapi::um::minwinbase::OVERLAPPED;

#[macro_use]
mod util;
use util::init_with_poll;

const DATA1: &[u8] = b"Hello world!";

pub fn raw(ov: &OVERLAPPED) -> *mut OVERLAPPED {
    ov as *const _ as *mut _
}

#[test]
fn test_async_file() {
    let (mut poll, mut events) = init_with_poll();

    let mut path = PathBuf::from("c:\\");
    path.push("work");
    path.push("git");
    path.push("mio");
    path.push("some_file.txt");
    let async_file = AsyncFile::open(path.as_path()).unwrap();

    // create a completion source to receive completion events for the async file
    let inner_arc = Arc::clone(&async_file.sys);
    let mut csh_test = CompletionSourceHandle::new(inner_arc);

    // register the completion source with the poll registry
    csh_test.associate_cp(poll.registry()).unwrap();

    // do an async write
    let overlapped: OVERLAPPED = unsafe { mem::zeroed() };
    unsafe {
        async_file.write(&DATA1, raw(&overlapped)).unwrap();
    }

    // this will return when async write completed
    poll.poll(&mut events, None).unwrap();

    // do an async read
    let mut buf = [0; 20];
    unsafe {
        async_file.read(&mut buf, raw(&overlapped)).unwrap();
    }

    // this will return when async read completed
    poll.poll(&mut events, None).unwrap();

    // check what we read is ok
    assert_eq!(&buf[..DATA1.len()], DATA1);
}
