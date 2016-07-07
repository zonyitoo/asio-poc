use std::io;
use std::os::unix::io::RawFd;
use std;

use nix::sys::socket::accept;
use nix::unistd::{read, write};
use nix;

use ::IoContext;
use ::net;

macro_rules! io_loop {
    ($this:expr, $io_op:expr, $ok_pat:pat, $ok_param:expr) => (
        loop {
            match $io_op {
                $ok_pat => ($this.func.take().unwrap())($ok_param),
                Err(nix::Error::Sys(nix::errno::EINTR)) => continue,
                Err(nix::Error::Sys(nix::errno::EAGAIN)) => return Ok(false),
                Err(err) => ($this.func.take().unwrap())(Err(io::Error::from(err))),
            }

            break;
        }

        return Ok(true);
    );
}


pub trait Operation {
    // Returns Ok(true) when the operation finished, and Ok(false) if it has to retry
    fn perform(&mut self, io_context: &IoContext) -> io::Result<bool>;
}

pub struct ReadOperation<'a, F>
    where F: FnOnce(io::Result<&mut [u8]>)
{
    fd: RawFd,
    func: Option<F>,
    buf: &'a mut [u8],
}

impl<'a, F> Operation for ReadOperation<'a, F>
    where F: FnOnce(io::Result<&mut [u8]>)
{
    fn perform(&mut self, io_context: &IoContext) -> io::Result<bool> {
        io_loop!(self, read(self.fd, self.buf), Ok(n), Ok(&mut self.buf[..n]));
    }
}

pub struct WriteOperation<'a, F>
    where F: FnOnce(io::Result<usize>)
{
    fd: RawFd,
    func: Option<F>,
    buf: &'a [u8],
}

impl<'a, F> Operation for WriteOperation<'a, F>
    where F: FnOnce(io::Result<usize>)
{
    fn perform(&mut self, io_context: &IoContext) -> io::Result<bool> {
        io_loop!(self, write(self.fd, self.buf), Ok(n), Ok(n as _));
    }
}

pub struct AcceptOperation<F>
    where F: FnOnce(io::Result<net::TcpStream>)
{
    fd: RawFd,
    func: Option<F>,
}

impl<F> Operation for AcceptOperation<F>
    where F: FnOnce(io::Result<net::TcpStream>)
{
    fn perform(&mut self, io_context: &IoContext) -> io::Result<bool> {
        io_loop!(self,
                 accept(self.fd),
                 Ok(fd),
                 unsafe { net::TcpStream::from_raw_fd(io_context, fd) });
    }
}
