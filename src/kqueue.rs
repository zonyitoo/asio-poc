use std::collections::LinkedList;
use std::mem;
use std::os::unix::io::RawFd;

use nix;
use nix::sys::event::*;
use nix::unistd::*;

use ::operation::Operation;

pub struct DescriptorState {
    wait_queue: [Vec<Box<Operation>>; 2], // 0 for read, 1 for write
}

pub struct Reactor {
    fd: RawFd,
}

impl Reactor {
    pub fn new() -> nix::Result<Reactor> {
        Ok(Reactor { fd: try!(kqueue()) })
    }

    pub fn register_socket(&self, fd: RawFd) -> nix::Result<Box<DescriptorState>> {
        let state = Box::new(DescriptorState { wait_queue: [Vec::new(), Vec::new()] });

        macro_rules! event {
            ($filter:expr) => (
                KEvent {
                    ident: fd as _,
                    filter: $filter,
                    flags: EV_ADD | EV_CLEAR,
                    fflags: FilterFlag::empty(),
                    data: 0,
                    udata: state.as_ref() as *const _ as _,
                }
            );
        }

        let changelist = [event!(EventFilter::EVFILT_READ), event!(EventFilter::EVFILT_WRITE)];
        kevent(self.fd, &changelist, &mut [], 0).and(Ok(state))
    }

    pub fn poll(&self, timeout: usize) -> nix::Result<()> {
        let mut eventlist: [KEvent; 128] = unsafe { mem::uninitialized() };
        let nevents = try!(kevent(self.fd, &[], &mut eventlist, timeout)) as usize;

        for event in &eventlist[..nevents] {
            let state = unsafe { &*(event.udata as *const DescriptorState) };
        }

        Ok(())
    }
}

impl Drop for Reactor {
    fn drop(&mut self) {
        let _ = close(self.fd);
    }
}
