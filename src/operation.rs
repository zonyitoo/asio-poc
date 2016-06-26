use nix::unistd::{read, write};
use std::io;

use std;
use nix;

pub trait Operation {
    // Returns Ok(true) when the operation finished, and Ok(false) if it has to retry
    fn perform(&mut self) -> io::Result<bool>;
}

pub struct ReadOperation<'a, F>
    where F: FnOnce(io::Result<&mut [u8]>)
{
    func: F,
    buf: &'a mut [u8],
}

impl<'a, F> Operation for ReadOperation<'a, F>
    where F: FnOnce(io::Result<&mut [u8]>)
{
    fn perform(&mut self) -> io::Result<bool> {
        loop {
            let fd = 0 as _;

            match read(fd, self.buf) {
                Ok(n) => {
                    (self.func)(Ok(&mut self.buf[..n]));
                    return Ok(true);
                }
                Err(nix::Error::Sys(nix::errno::EINTR)) => continue,
                Err(nix::Error::Sys(nix::errno::EAGAIN)) |
                Err(nix::Error::Sys(nix::errno::EWOULDBLOCK)) => return Ok(false),
                Err(err) => {
                    (self.func)(Err(io::Error::from(err)));
                    return Ok(true);
                }
            }
        }
    }
}
