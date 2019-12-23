use super::Event;
use std::mem::transmute;
use winapi::um::minwinbase::OVERLAPPED_ENTRY;

// TODO: this function should receive some context about what selector
// is calling it.
// TODO: with the current locking scheme, it'd be more efficient to handle
// completions in bulk.
pub type CompletionHandler2 = fn(&'_ OVERLAPPED_ENTRY) -> Option<Event>;

pub fn as_key(handler: CompletionHandler2) -> usize {
    unsafe { transmute(handler) }
}

pub unsafe fn from_key(key: usize) -> CompletionHandler2 {
    transmute(key)
}
