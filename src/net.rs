use std::io;
use std::net::ToSocketAddrs;
use std::os::unix::io::{RawFd, FromRawFd};

use ::IoContext;

fn set_nonblock(fd: RawFd) -> nix::Result<()> {
    let flags = try!(fcntl(fd, FcntlArg::F_GETFL));
    let mut flags = OFlag::from_bits_truncate(flags);
    flags.insert(O_NONBLOCK);
    fcntl(fd, FcntlArg::F_SETFL(flags)).and(Ok(()))
}

struct TcpSocket {
    fd: RawFd,
}

impl FromRawFd for TcpSocket {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        let socket = TcpSocket { fd: fd };
    }
}

struct TcpListener<'a> {
    io_context: &'a IoContext,
    fd: RawFd,
}

impl<'a> TcpListener<'a> {
    fn bind<A: ToSocketAddrs>(io_context: &'a IoContext, addrs: A) -> io::Result<TcpListener> {
        let fd = try!(socket(AddressFamily::Inet, SockType::Stream, SockFlag::empty(), 0));
        try!(set_nonblock(fd));

        let listener = TcpListener {
            io_context: io_context,
            fd: fd,
        };

        for addr in addrs.to_socket_addrs() {
            try!(bind(fd, &addr));
        }

        try!(listen(fd, 128));
        try!(io_context.register_socket(fd, EventFilter::EVFILT_READ));

        Ok(listener)
    }

    fn accept_async<F>(&self, func: F)
        where F: FnOnce(io::Result<(TcpStream, SocketAddr)>)
    {
        ;
    }
}

impl FromRawFd for TcpListener {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        let socket = TcpListener { fd: fd };
    }
}
