use crate::{event, poll, Registry};

use std::os::windows::io::{AsRawHandle,  RawHandle};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::{fmt, io};

use winapi::um::minwinbase::OVERLAPPED_ENTRY;
use miow::iocp::CompletionPort;

pub type CompletionHandler = fn(&'_ OVERLAPPED_ENTRY) -> Option<bool>;

pub trait HasCompletion {
    fn complete(&self, oe: &OVERLAPPED_ENTRY) -> Option<bool>;
    fn get_raw_handle(&self) -> Arc<RawHandle>;
    fn get_completion_handler(&self) -> CompletionHandler;
}

pub struct AssociatedCSHState {
    cp: Arc<CompletionPort>,
    handle: Arc<RawHandle>, 
    completion_handler: CompletionHandler,
    }

impl AssociatedCSHState {
    pub fn new(cp: Arc<CompletionPort>, rh: Arc<RawHandle>, ch: CompletionHandler) -> AssociatedCSHState {
        AssociatedCSHState { cp: Arc::clone(&cp), handle: rh, completion_handler: ch, }
    }
}

impl fmt::Debug for AssociatedCSHState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssociatedCSHState").finish()
    }
}

/// abc
#[derive(Debug)]
pub struct CompletionSourceHandle<T> {
    state: Option<Pin<Arc<Mutex<AssociatedCSHState>>>>,
    inner: Arc<T>,
}

/// abc
impl <T: HasCompletion + AsRawHandle> CompletionSourceHandle<T> {
    /// abc
    pub fn new(t: Arc<T>) -> CompletionSourceHandle<T> {
        CompletionSourceHandle {
            state: None,
            inner: t,
        }
    }

    /// abc
    pub fn get_state(&self) -> Option<Pin<Arc<Mutex<AssociatedCSHState>>>> {
        match &self.state {
            Some(state) => Some(state.clone()),
            None => None,
        }
    }

    /// abc
    pub fn set_state(&mut self, state: Option<Pin<Arc<Mutex<AssociatedCSHState>>>>) {
        self.state = state
    }


    /// abc
    pub fn get_completion_source(&self) -> Arc<T> {
        self.inner.clone()
    }
}

impl<T: HasCompletion + AsRawHandle> event::CompletionSource for CompletionSourceHandle<T> {

    /// abc
    fn associate_cp(
        &mut self,
        registry: &Registry,
    ) -> io::Result<()> {
        poll::selector(registry).associate_cp(self)
    }
}

