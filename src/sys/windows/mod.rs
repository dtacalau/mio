use std::pin::Pin;
use std::sync::{Arc, Mutex};

mod afd;
mod io_status_block;
pub mod event;
pub use event::{Event, Events};

mod selector;
pub use selector::{Selector, SelectorInner};

mod sock_selector;
pub use sock_selector::{SockSelector, SockState};

mod completion_handler;

use winapi::shared::ntdef::PVOID;
use winapi::um::minwinbase::{OVERLAPPED};

// Macros must be defined before the modules that use them
cfg_net! {
    /// Helper macro to execute a system call that returns an `io::Result`.
    //
    // Macro must be defined before any modules that uses them.
    macro_rules! syscall {
        ($fn: ident ( $($arg: expr),* $(,)* ), $err_test: path, $err_value: expr) => {{
            let res = unsafe { $fn($($arg, )*) };
            if $err_test(&res, &$err_value) {
                Err(io::Error::last_os_error())
            } else {
                Ok(res)
            }
        }};
    }

    /// Helper macro to execute an I/O operation and register interests if the
    /// operation would block.
    macro_rules! try_io {
        ($self: ident, $method: ident $(, $args: expr)*)  => {{
            let result = (&$self.inner).$method($($args),*);
            if let Err(ref e) = result {
                if e.kind() == io::ErrorKind::WouldBlock {
                    $self.io_blocked_reregister()?;
                }
            }
            result
        }};
    }
}

cfg_tcp! {
    mod tcp;
    pub use tcp::{TcpListener, TcpStream};
}

cfg_udp! {
    mod udp;
    pub use udp::UdpSocket;

    mod completion_source;
    pub use completion_source::HasCompletion;
    pub use completion_source::CompletionHandler;
    pub use completion_source::CompletionSourceHandle;
    pub use completion_source::AssociatedCSHState;

    mod async_file;
    pub use async_file::AsyncFile;
}

mod waker;
pub use waker::Waker;

pub trait SocketState {
    // The `SockState` struct needs to be pinned in memory because it contains
    // `OVERLAPPED` and `AFD_POLL_INFO` fields which are modified in the
    // background by the windows kernel, therefore we need to ensure they are
    // never moved to a different memory address.
    fn get_sock_state(&self) -> Option<Pin<Arc<Mutex<SockState>>>>;
    fn set_sock_state(&self, sock_state: Option<Pin<Arc<Mutex<SockState>>>>);
}

cfg_net! {
    use crate::{Interest, Token};
    use std::io;
    use std::mem::size_of_val;
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
    use std::sync::Once;
    use winapi::ctypes::c_int;
    use winapi::shared::ws2def::SOCKADDR;
    use winapi::um::winsock2::{
        ioctlsocket, socket, FIONBIO, INVALID_SOCKET, PF_INET, PF_INET6, SOCKET,
    };

    struct InternalState {
        selector: Arc<SockSelector>,
        token: Token,
        interests: Interest,
        sock_state: Option<Pin<Arc<Mutex<SockState>>>>,
    }

    impl InternalState {
        fn new(selector: Arc<SockSelector>, token: Token, interests: Interest) -> InternalState {
            InternalState {
                selector,
                token,
                interests,
                sock_state: None,
            }
        }
    }

    impl Drop for InternalState {
        fn drop(&mut self) {
            if let Some(sock_state) = self.sock_state.as_ref() {
                let mut sock_state = sock_state.lock().unwrap();
                sock_state.mark_delete();
            }
        }
    }

    /// Initialise the network stack for Windows.
    fn init() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            // Let standard library call `WSAStartup` for us, we can't do it
            // ourselves because otherwise using any type in `std::net` would panic
            // when it tries to call `WSAStartup` a second time.
            drop(std::net::UdpSocket::bind("127.0.0.1:0"));
        });
    }

    /// Create a new non-blocking socket.
    fn new_socket(addr: SocketAddr, socket_type: c_int) -> io::Result<SOCKET> {
        let domain = match addr {
            SocketAddr::V4(..) => PF_INET,
            SocketAddr::V6(..) => PF_INET6,
        };

        syscall!(
            socket(domain, socket_type, 0),
            PartialEq::eq,
            INVALID_SOCKET
        )
        .and_then(|socket| {
            syscall!(ioctlsocket(socket, FIONBIO, &mut 1), PartialEq::ne, 0).map(|_| socket as SOCKET)
        })
    }

    fn socket_addr(addr: &SocketAddr) -> (*const SOCKADDR, c_int) {
        match addr {
            SocketAddr::V4(ref addr) => (
                addr as *const _ as *const SOCKADDR,
                size_of_val(addr) as c_int,
            ),
            SocketAddr::V6(ref addr) => (
                addr as *const _ as *const SOCKADDR,
                size_of_val(addr) as c_int,
            ),
        }
    }

    fn inaddr_any(other: SocketAddr) -> SocketAddr {
        match other {
            SocketAddr::V4(..) => {
                let any = Ipv4Addr::new(0, 0, 0, 0);
                let addr = SocketAddrV4::new(any, 0);
                SocketAddr::V4(addr)
            }
            SocketAddr::V6(..) => {
                let any = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0);
                let addr = SocketAddrV6::new(any, 0, 0, 0);
                SocketAddr::V6(addr)
            }
        }
    }

    /// Converts the pointer to a `SockState` into a raw pointer.
    /// To revert see `from_overlapped`.
    fn into_overlapped(sock_state: Pin<Arc<Mutex<SockState>>>) -> PVOID {
        let overlapped_ptr: *const Mutex<SockState> =
            unsafe { Arc::into_raw(Pin::into_inner_unchecked(sock_state)) };
        overlapped_ptr as *mut _
    }

    /// Convert a raw overlapped pointer into a reference to `SockState`.
    /// Reverts `into_overlapped`.
    fn from_overlapped(ptr: *mut OVERLAPPED) -> Pin<Arc<Mutex<SockState>>> {
        let sock_ptr: *const Mutex<SockState> = ptr as *const _;
        unsafe { Pin::new_unchecked(Arc::from_raw(sock_ptr)) }
    }
}
