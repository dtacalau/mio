use super::completion_handler;
use super::Event;
use super::from_overlapped;
use super::SockSelector;
use crate::sys::Events;
use crate::Interest;

use miow::iocp::{CompletionPort, CompletionStatus};
#[cfg(debug_assertions)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{Ordering};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{io};
use winapi::shared::winerror::{WAIT_TIMEOUT};

use std::collections::{VecDeque, HashSet};


/// Each Selector has a globally unique(ish) ID associated with it. This ID
/// gets tracked by `TcpStream`, `TcpListener`, etc... when they are first
/// registered with the `Selector`. If a type that is previously associated with
/// a `Selector` attempts to register itself with a different `Selector`, the
/// operation will return with an error. This matches windows behavior.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// Windows implementaion of `sys::Selector`
///
/// Edge-triggered event notification is simulated by resetting internal event flag of each socket state `SockState`
/// and setting all events back by intercepting all requests that could cause `io::ErrorKind::WouldBlock` happening.
///
/// This selector is currently only support socket due to `Afd` driver is winsock2 specific.
#[derive(Debug)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,

    inner: Arc<SelectorInner>,
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        SelectorInner::new().map(|inner| {
            #[cfg(debug_assertions)]
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed) + 1;
            Selector {
                #[cfg(debug_assertions)]
                id,
                inner: Arc::new(inner),
            }
        })
    }

    //TODO: not really ok to return this here because there could be multiple of them,
    // how to return the one we need?
    pub(super) fn sock_selector(&self) -> Arc<SockSelector> {
        self.inner.sock_selector()
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(Selector {
            #[cfg(debug_assertions)]
            id: self.id,
            inner: Arc::clone(&self.inner),
        })
    }

    /// # Safety
    ///
    /// This requires a mutable reference to self because only a single thread
    /// can poll IOCP at a time.
    pub fn select(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        self.inner.select(events, timeout)
    }

    pub(super) fn clone_port(&self) -> Arc<CompletionPort> {
        self.inner.cp.clone()
    }
}

cfg_net! {
    use super::SocketState;
    use super::HasCompletion;
    use super::Complete;
    use super::CompletionHandler;
    use super::CompletionSourceHandle;
    use super::AssociatedCSHState;
    use super::IocpResource;
    use crate::Token;
    use std::os::windows::io::{AsRawHandle, AsRawSocket, RawHandle};

    impl Selector {

        pub fn register_cp_handle<H, T>(
            &self,
            io_handler: Arc<H>,
            iocp_resource: IocpResource<T>
        ) -> io::Result<()> 
        where H: AsRawHandle, T: Complete, {
            self.inner.register_cp_handle(io_handler, iocp_resource)
        }

        pub fn associate_cp<T>(
            &self,
            csh: &mut CompletionSourceHandle<T>,
        ) -> io::Result<()> 
        where T: HasCompletion + AsRawHandle, {
            self.inner.associate_cp(csh)
        }

        pub fn dissociate_cp<T>(
            &self,
            csh: &mut CompletionSourceHandle<T>,
        ) -> io::Result<()> 
        where T: HasCompletion + AsRawHandle, {
            self.inner.dissociate_cp(csh)
        }

        pub fn register<S: SocketState + AsRawSocket>(
            &self,
            socket: &S,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            self.inner.register(socket, token, interests)
        }

        pub fn reregister<S: SocketState>(
            &self,
            socket: &S,
            token: Token,
            interests: Interest,
        ) -> io::Result<()> {
            self.inner.reregister(socket, token, interests)
        }

        pub fn deregister<S: SocketState>(&self, socket: &S) -> io::Result<()> {
            self.inner.deregister(socket)
        }

        #[cfg(debug_assertions)]
        pub fn id(&self) -> usize {
            self.id
        }
    }
}

#[derive(Debug)]
pub struct SelectorInner {
    cp: Arc<CompletionPort>,
    sock_selector: Arc<SockSelector>,
    csh_state_queue: Mutex<VecDeque<Pin<Arc<Mutex<AssociatedCSHState>>>>>,
    iocp_handle_set: HashSet<RawHandle>,
}

// We have ensured thread safety by introducing lock manually.
unsafe impl Sync for SelectorInner {}
use std::mem::transmute;

impl SelectorInner {
    pub fn new() -> io::Result<SelectorInner> {
        CompletionPort::new(0).map(|cp| {
            let cp = Arc::new(cp);
            let cp_afd = Arc::clone(&cp);
            SelectorInner {
                cp,
                sock_selector: Arc::new(SockSelector::new(cp_afd)),
                csh_state_queue: Mutex::new(VecDeque::new()),
                iocp_handle_set: HashSet::<RawHandle>::new(),
            }
        })
    }

    pub fn as_key(handler: CompletionHandler) -> usize {
        unsafe { transmute(handler) }
    }

    pub fn register_cp_handle<H, T>(
        &self,
        io_handler: Arc<H>,
        iocp_resource: IocpResource<T>
    ) -> io::Result<()> 
    where H: AsRawHandle, T: Complete, {
/*
        if self.iocp_handle_set.contains(&io_handler) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));   
        }*/

        let key = iocp_resource.get_completion_handler();
        self.cp.add_handle(key, &*io_handler).unwrap();

       // self.iocp_handle_set.insert(*io_handler);

        Ok(())
    }
    
    pub fn associate_cp<T> (
        &self,
        csh: &mut CompletionSourceHandle<T>,
    ) -> io::Result<()> 
    where T: HasCompletion + AsRawHandle, {
        let completion_source = csh.get_completion_source();
        let csh_state = csh.get_state();
        
        match csh_state {
            Some(state)  => {
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
        
            None => {
            let key = Self::as_key(completion_source.get_completion_handler());
            self.cp.add_handle(key, &(*completion_source)).unwrap();

            let state_arc = Arc::pin(Mutex::new(
                AssociatedCSHState::new(Arc::clone(&self.cp), completion_source.get_raw_handle(), completion_source.get_completion_handler())));
            self.csh_state_queue.lock().unwrap().push_back(state_arc.clone());

            csh.set_state(Some(state_arc));
            }
        }

        //TODO: handle error case here
        Ok(())
    }

    pub fn dissociate_cp<T>(
        &self,
        csh: &mut CompletionSourceHandle<T>,
    ) -> io::Result<()>
    where T: HasCompletion + AsRawHandle,  {
        //let csh_state_queue = self.csh_state_queue.lock().unwrap();
        //let csource_handle = csource.get_handle().unwrap();
        //TODO: do a proper delete here or some kind of removal
        //self.csh_state_queue.delete(csource_handle);
        //csh_state.dissociate_cp(Arc::clone(&self.cp))
        Ok(())
    }

    pub fn sock_selector(&self) -> Arc<SockSelector> {
        self.sock_selector.clone()
    }

    pub fn register<S: SocketState + AsRawSocket>(
        &self,
        socket: &S,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.sock_selector().register(socket, token, interests)
    }

    pub fn reregister<S: SocketState>(
        &self,
        socket: &S,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        self.sock_selector().reregister(socket, token, interests)
    }

    pub fn deregister<S: SocketState>(&self, socket: &S) -> io::Result<()> {
        self.sock_selector().deregister(socket)
    }

    /// # Safety
    ///
    /// May only be calling via `Selector::select`.
    pub fn select(&self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        if timeout.is_none() {
            loop {
                let len = self.select2(&mut events.statuses, &mut events.events, None)?;
                if len == 0 {
                    continue;
                }
                return Ok(());
            }
        } else {
            self.select2(&mut events.statuses, &mut events.events, timeout)?;
            return Ok(());
        }
    }

    pub fn select2(
        &self,
        statuses: &mut [CompletionStatus],
        events: &mut Vec<Event>,
        timeout: Option<Duration>,
    ) -> io::Result<usize> {
        // TODO: make a before-poll hook available to external handlers.
        self.sock_selector.notify_poll_start()?;

        let result = self.cp.get_many(statuses, timeout);

        // TODO: make after-poll notification available to external handlers.
        self.sock_selector.notify_poll_end()?;


        /*self.is_polling.store(false, Ordering::Relaxed);*/

        match result {
            Ok(iocp_events) => Ok(unsafe { 
            /*    self.feed_events(events, iocp_events) */
            for completion in iocp_events.iter().map(CompletionStatus::entry) {
                let handler =  completion_handler::from_key(completion.lpCompletionKey);
                match handler(&completion) {
                    Some(e) => events.push(e),
                    None => {}
                };
            }
            events.iter().count()
            }),
            Err(ref e) if e.raw_os_error() == Some(WAIT_TIMEOUT as i32) => Ok(0),
            Err(e) => Err(e),
        }
    }
}

impl Drop for SelectorInner {
    fn drop(&mut self) {
        loop {
            let events_num: usize;
            let mut statuses: [CompletionStatus; 1024] = [CompletionStatus::zero(); 1024];

            let result = self
                .cp
                .get_many(&mut statuses, Some(std::time::Duration::from_millis(0)));
            match result {
                Ok(iocp_events) => {
                    events_num = iocp_events.iter().len();
                    for iocp_event in iocp_events.iter() {
                        if !iocp_event.overlapped().is_null() {
                            // drain sock state to release memory of Arc reference
                            let _sock_state = from_overlapped(iocp_event.overlapped());
                        }
                    }
                }

                Err(_) => {
                    break;
                }
            }

            if events_num == 0 {
                // continue looping until all completion statuses have been drained
                break;
            }
        }

        //TODO: how to handle this w/o having afd knowledge
       //temp disabled self.afd_group.release_unused_afd();
    }
}
