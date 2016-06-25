extern crate nix;

use std::os::unix::io::RawFd;
use std::mem;
use std::collections::{VecDeque, HashMap};

use nix::sys::socket::*;
use nix::unistd::*;
use nix::sys::event::*;
use nix::fcntl::*;

fn set_nonblock(fd: RawFd) -> nix::Result<()> {
    let flags = try!(fcntl(fd, FcntlArg::F_GETFL));
    let mut flags = OFlag::from_bits_truncate(flags);
    flags.insert(O_NONBLOCK);
    fcntl(fd, FcntlArg::F_SETFL(flags)).and(Ok(()))
}

struct Kqueue {
    fd: RawFd,
}

impl Kqueue {
    fn new() -> nix::Result<Kqueue> {
        Ok(Kqueue { fd: try!(kqueue()) })
    }

    fn add_ev(&self, fd: RawFd, filter: EventFilter) -> nix::Result<()> {
        let ev = KEvent {
            ident: fd as _,
            filter: filter,
            flags: EV_ADD | EV_ENABLE,
            fflags: FilterFlag::empty(),
            data: 0,
            udata: fd as _,
        };
        let changelist: [KEvent; 1] = [ev];
        let mut eventlist: [KEvent; 0] = [];
        kevent(self.fd, &changelist, &mut eventlist, 0).and(Ok(()))
    }

    fn del_ev(&self, fd: RawFd, filter: EventFilter) -> nix::Result<()> {
        let ev = KEvent {
            ident: fd as _,
            filter: filter,
            flags: EV_DELETE,
            fflags: FilterFlag::empty(),
            data: 0,
            udata: fd as _,
        };
        let changelist: [KEvent; 1] = [ev];
        let mut eventlist: [KEvent; 0] = [];
        kevent(self.fd, &changelist, &mut eventlist, 0).and(Ok(()))
    }

    fn poll(&self, eventlist: &mut [KEvent], timeout: usize) -> nix::Result<usize> {
        kevent(self.fd, &[], eventlist, timeout).map(|n| n as usize)
    }
}

impl Drop for Kqueue {
    fn drop(&mut self) {
        let _ = close(self.fd);
    }
}

struct Client {
    buf: VecDeque<u8>,
    fd: RawFd,
}

impl Client {
    fn new(cfd: RawFd) -> nix::Result<Client> {
        try!(set_nonblock(cfd));
        Ok(Client {
            buf: VecDeque::with_capacity(1024),
            fd: cfd,
        })
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = close(self.fd);
    }
}

struct Server {
    kqueue: Kqueue,
    lfd: RawFd,
    clients: HashMap<RawFd, Client>,
}

impl Server {
    fn new(addr: SockAddr) -> nix::Result<Server> {
        let kqueue = try!(Kqueue::new());
        let lfd = try!(socket(AddressFamily::Inet, SockType::Stream, SockFlag::empty(), 0));
        try!(set_nonblock(lfd));

        try!(bind(lfd, &addr));
        try!(listen(lfd, 128));

        try!(kqueue.add_ev(lfd, EventFilter::EVFILT_READ));

        Ok(Server {
            kqueue: kqueue,
            lfd: lfd,
            clients: HashMap::new(),
        })
    }

    fn run_once(&mut self, timeout: usize) -> nix::Result<()> {
        let mut evs: [KEvent; 128] = unsafe { mem::uninitialized() };
        let n = try!(self.kqueue.poll(&mut evs, timeout));

        for ev in &evs[..n] {
            let fd = ev.udata as RawFd;

            match ev.filter {
                EventFilter::EVFILT_READ => {
                    if fd == self.lfd {
                        match self.handle_accept() {
                            Ok(..) => {}
                            Err(..) => {
                                let _ = self.close_client(fd);
                            }
                        }
                    } else {
                        match self.handle_read(fd) {
                            Ok(..) => {}
                            Err(..) => {
                                let _ = self.close_client(fd);
                            }
                        }
                    }
                }
                EventFilter::EVFILT_WRITE => {
                    match self.handle_write(fd) {
                        Ok(..) => {}
                        Err(..) => {
                            let _ = self.close_client(fd);
                        }
                    }
                }
                ev => {
                    panic!("Unknown event: {:?}", ev);
                }
            }
        }

        Ok(())
    }

    fn handle_accept(&mut self) -> nix::Result<()> {
        loop {
            match accept(self.lfd) {
                Ok(fd) => {
                    let c = try!(Client::new(fd));
                    try!(self.kqueue.add_ev(fd, EventFilter::EVFILT_READ));
                    // try!(add_ev(self.kfd, fd, EventFilter::EVFILT_WRITE));
                    self.clients.insert(fd, c);

                    println!("[ TRACE ] Accepted client {:?}", fd);
                }
                Err(nix::Error::Sys(nix::Errno::EAGAIN)) => {
                    break;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    fn handle_read(&mut self, cfd: RawFd) -> nix::Result<()> {
        let mut should_close = false;
        {
            let client = match self.clients.get_mut(&cfd) {
                Some(c) => c,
                None => return Ok(()),
            };

            loop {
                let mut buf = [0u8; 1024];
                match read(client.fd, &mut buf) {
                    Ok(0) => {
                        should_close = true;
                        println!("[ TRACE ] Client {:?} got EOF", client.fd);
                        break;
                    }
                    Ok(n) => {
                        client.buf.extend(buf[..n].iter());
                        println!("[ TRACE ] Client {:?} read {:?} bytes", client.fd, n);
                    }
                    Err(nix::Error::Sys(nix::Errno::EAGAIN)) => {
                        break;
                    }
                    Err(e) => {
                        // !!
                        should_close = true;
                        println!("[ ERROR ] Client {:?} read {:?}", client.fd, e);
                        break;
                    }
                }
            }
        }

        if should_close {
            try!(self.close_client(cfd));
        } else {
            try!(self.kqueue.del_ev(cfd, EventFilter::EVFILT_READ));
            try!(self.kqueue.add_ev(cfd, EventFilter::EVFILT_WRITE));
        }

        Ok(())
    }

    fn handle_write(&mut self, cfd: RawFd) -> nix::Result<()> {
        let mut should_close = false;
        {
            let client = match self.clients.get_mut(&cfd) {
                Some(c) => c,
                None => return Ok(()),
            };

            loop {
                let (fd, r) = {
                    let (buf, _) = client.buf.as_slices();
                    if buf.len() == 0 {
                        break;
                    }
                    (client.fd, write(client.fd, buf))
                };

                match r {
                    Ok(0) => {
                        should_close = true;
                        println!("[ TRACE ] Client {:?} got EOF", fd);
                        break;
                    }
                    Ok(n) => {
                        client.buf.drain(..n);
                        println!("[ TRACE ] Client {:?} write {:?} bytes", fd, n);
                    }
                    Err(nix::Error::Sys(nix::Errno::EAGAIN)) => {
                        break;
                    }
                    Err(e) => {
                        // !!
                        should_close = true;
                        println!("[ ERROR ] Client {:?} read {:?}", fd, e);
                        break;
                    }
                }
            }
        }

        if should_close {
            try!(self.close_client(cfd))
        } else {
            try!(self.kqueue.add_ev(cfd, EventFilter::EVFILT_READ));
            try!(self.kqueue.del_ev(cfd, EventFilter::EVFILT_WRITE));
        }

        Ok(())
    }

    fn close_client(&mut self, cfd: RawFd) -> nix::Result<()> {
        try!(self.kqueue.del_ev(cfd, EventFilter::EVFILT_READ));
        try!(self.kqueue.del_ev(cfd, EventFilter::EVFILT_WRITE));

        self.clients.remove(&cfd);
        println!("[ TRACE ] Client {:?} closed", cfd);

        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = close(self.lfd);
    }
}

fn main() {
    let addr = SockAddr::new_inet(InetAddr::new(IpAddr::new_v4(127, 0, 0, 1), 3000));
    let mut server = Server::new(addr).unwrap();

    println!("[ TRACE ] Server running ...");
    while let Ok(..) = server.run_once(0) {}
}
