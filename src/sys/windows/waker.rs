use crate::sys::windows::Selector;
use crate::Token;
use super::afd::POLL_RECEIVE;
use super::completion_handler;
use super::{Event};

use miow::iocp::{CompletionPort, CompletionStatus};
use miow::Overlapped;
use winapi::um::minwinbase::OVERLAPPED_ENTRY;
use std::io;
use std::sync::Arc;

#[derive(Debug)]
pub struct Waker {
    token: Token,
    port: Arc<CompletionPort>,
}

impl Waker {
    pub fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        Ok(Waker {
            token,
            port: selector.clone_port(),
        })
    }

    pub fn wake(&self) -> io::Result<()> {
        // Keep NULL as Overlapped value to notify waking.
        let key = completion_handler::as_key(Self::handle_completion);
        let overlapped = self.token.0 as *mut Overlapped;
        let status = CompletionStatus::new(0, key, overlapped);

        self.port.post(status)
    }

    fn handle_completion(completion: &OVERLAPPED_ENTRY) -> Option<Event> {
        Some(Event {
            flags: POLL_RECEIVE, // TODO: why use an AFD flag here?
            data: completion.lpOverlapped as u64,
        })
    }
}
