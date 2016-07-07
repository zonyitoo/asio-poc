use nix;
use std::io;
use std::os::unix::io::RawFd;

use {Reactor, DescriptorState};

#[inline]
fn into_io_result<T>(result: nix::Result<T>) -> io::Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(v) => Err(io::Error::from(v)),
    }
}

pub struct IoContext {
    reactor: Reactor,
}

impl IoContext {
    fn new() -> io::Result<IoContext> {
        Ok(IoContext { reactor: try!(Reactor::new()) })
    }

    pub fn register_socket(&self, fd: RawFd) -> io::Result<Box<DescriptorState>> {
        into_io_result(self.reactor.register_socket(fd))
    }

    fn run(&self) -> io::Result<()> {
        loop {
            try!(self.reactor.poll(300000))
        }

        Ok(())
    }
}
