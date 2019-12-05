use super::{socket_addr, SocketAddr};
use crate::sys::unix::net::new_socket;
use crate::unix::SourceFd;
use crate::{event, Interest, Registry, Token};

use std::io;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::net;
use std::path::Path;

#[derive(Debug)]
pub struct UnixDatagram {
    inner: net::UnixDatagram,
}

impl UnixDatagram {
    fn new(inner: net::UnixDatagram) -> UnixDatagram {
        UnixDatagram { inner }
    }

    pub(crate) fn bind(path: &Path) -> io::Result<UnixDatagram> {
        let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
        let (sockaddr, socklen) = socket_addr(path)?;
        let sockaddr = &sockaddr as *const libc::sockaddr_un as *const libc::sockaddr;

        syscall!(bind(socket, sockaddr, socklen))?;
        Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
    }

    pub fn from_std(inner: net::UnixDatagram) -> UnixDatagram {
        UnixDatagram { inner }
    }

    pub(crate) fn connect<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        self.inner.connect(path)
    }

    pub(crate) fn pair() -> io::Result<(UnixDatagram, UnixDatagram)> {
        let mut fds = [-1; 2];
        let flags = libc::SOCK_DGRAM;
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "solaris")))]
        let flags = flags | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC;

        syscall!(socketpair(libc::AF_UNIX, flags, 0, fds.as_mut_ptr()))?;
        let pair = unsafe {
            (
                UnixDatagram::from_raw_fd(fds[0]),
                UnixDatagram::from_raw_fd(fds[1]),
            )
        };

        // Darwin and Solaris do not have SOCK_NONBLOCK or SOCK_CLOEXEC.
        //
        // In order to set those flags, additional `fcntl` sys calls must be
        // performed. If a `fnctl` fails after the sockets have been created,
        // the file descriptors will leak. Creating `pair` above ensures that
        // if there is an error, the file descriptors are closed.
        #[cfg(any(target_os = "ios", target_os = "macos", target_os = "solaris"))]
        {
            syscall!(fcntl(fds[0], libc::F_SETFL, libc::O_NONBLOCK))?;
            syscall!(fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC))?;
            syscall!(fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK))?;
            syscall!(fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC))?;
        }
        Ok(pair)
    }

    pub(crate) fn unbound() -> io::Result<UnixDatagram> {
        let socket = new_socket(libc::AF_UNIX, libc::SOCK_DGRAM)?;
        Ok(unsafe { UnixDatagram::from_raw_fd(socket) })
    }

    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            syscall!(getsockname(self.inner.as_raw_fd(), sockaddr, socklen))
        })
    }

    pub(crate) fn peer_addr(&self) -> io::Result<SocketAddr> {
        SocketAddr::new(|sockaddr, socklen| {
            syscall!(getpeername(self.inner.as_raw_fd(), sockaddr, socklen))
        })
    }

    pub(crate) fn recv_from(&self, dst: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut count = 0;
        let socketaddr = SocketAddr::new(|sockaddr, socklen| {
            syscall!(recvfrom(
                self.inner.as_raw_fd(),
                dst.as_mut_ptr() as *mut _,
                dst.len(),
                0,
                sockaddr,
                socklen,
            ))
            .map(|c| {
                count = c;
                c as libc::c_int
            })
        })?;
        Ok((count as usize, socketaddr))
    }

    pub(crate) fn recv(&self, dst: &mut [u8]) -> io::Result<usize> {
        self.inner.recv(dst)
    }

    pub(crate) fn send_to<P: AsRef<Path>>(&self, src: &[u8], path: P) -> io::Result<usize> {
        self.inner.send_to(src, path)
    }

    pub(crate) fn send(&self, src: &[u8]) -> io::Result<usize> {
        self.inner.send(src)
    }

    pub(crate) fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.inner.take_error()
    }

    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.inner.shutdown(how)
    }
}

impl event::Source for UnixDatagram {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

impl AsRawFd for UnixDatagram {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl FromRawFd for UnixDatagram {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixDatagram {
        UnixDatagram::new(net::UnixDatagram::from_raw_fd(fd))
    }
}

impl IntoRawFd for UnixDatagram {
    fn into_raw_fd(self) -> RawFd {
        self.inner.into_raw_fd()
    }
}
