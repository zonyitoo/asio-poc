use std::io;
use std::net;
use std::os::unix::io::{RawFd, FromRawFd};

use nix::fcntl;
use nix::sys::socket;
use nix;

use ::IoContext;

fn set_nonblock(fd: RawFd) -> io::Result<()> {
    let flags = try!(fcntl::fcntl(fd, fcntl::FcntlArg::F_GETFL));

    let mut flags = fcntl::OFlag::from_bits_truncate(flags);
    flags.insert(fcntl::O_NONBLOCK);

    try!(fcntl::fcntl(fd, fcntl::FcntlArg::F_SETFL(flags)));

    Ok(())
}

pub struct TcpStream<'ctx> {
    io_context: &'ctx IoContext,
    fd: RawFd,
}

impl<'ctx> TcpStream<'ctx> {
    pub unsafe fn from_raw_fd(io_context: &'ctx IoContext, fd: RawFd) -> io::Result<TcpStream> {
        let socket = TcpStream {
            io_context: io_context,
            fd: fd,
        };

        try!(set_nonblock(fd));
        try!(io_context.register_socket(fd));

        Ok(socket)
    }

    pub fn async_read(&self) {}
}

pub struct TcpListener<'ctx> {
    io_context: &'ctx IoContext,
    fd: RawFd,
}

impl<'ctx> TcpListener<'ctx> {
    pub fn bind<A: net::ToSocketAddrs>(io_context: &'ctx IoContext,
                                       addrs: A)
                                       -> io::Result<TcpListener> {

        let fd = try!(socket::socket(socket::AddressFamily::Inet,
                                     socket::SockType::Stream,
                                     socket::SockFlag::empty(),
                                     0));
        let listener = TcpListener {
            io_context: io_context,
            fd: fd,
        };

        try!(set_nonblock(fd));

        for addr in addrs.to_socket_addrs().unwrap() {
            let addr = socket::SockAddr::Inet(socket::InetAddr::from_std(&addr));
            try!(socket::bind(fd, &addr));
        }

        try!(socket::listen(fd, 128));
        try!(io_context.register_socket(fd));

        Ok(listener)
    }

    pub fn accept_async<F>(&self, func: F)
        where F: FnOnce(io::Result<(TcpStream, net::SocketAddr)>)
    {
        ;
    }
}
