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
    try!(fcntl(fd, FcntlArg::F_SETFL(flags)));

    Ok(())
}

fn add_ev(kfd: RawFd, fd: RawFd, filter: EventFilter) -> nix::Result<()> {
    let mut evs: [KEvent; 1] = unsafe { mem::uninitialized() };
    ev_set(&mut evs[0],
           fd as usize,
           filter,
           EV_ADD | EV_ENABLE,
           FilterFlag::empty(),
           fd as usize);

    let mut elist: [KEvent; 0] = [];
    let r = try!(kevent(kfd, &evs, &mut elist, 0));
    assert_eq!(r, 0);
    Ok(())
}

fn del_ev(kfd: RawFd, fd: RawFd, filter: EventFilter) -> nix::Result<()> {
    let mut evs: [KEvent; 1] = unsafe { mem::uninitialized() };
    ev_set(&mut evs[0],
           fd as usize,
           filter,
           EV_DELETE,
           FilterFlag::empty(),
           fd as usize);

    let r = try!(kevent(kfd, &evs, &mut [], 0));
    assert_eq!(r, 0);
    Ok(())
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
    kfd: RawFd,
    lfd: RawFd,
    clients: HashMap<RawFd, Client>,
}

impl Server {
    fn new(addr: SockAddr) -> nix::Result<Server> {
        let kfd = try!(kqueue());
        let lfd = try!(socket(AddressFamily::Inet, SockType::Stream, SockFlag::empty(), 0));
        try!(set_nonblock(lfd));

        try!(bind(lfd, &addr));
        try!(listen(lfd, 128));

        try!(add_ev(kfd, lfd, EventFilter::EVFILT_READ));

        Ok(Server {
            kfd: kfd,
            lfd: lfd,
            clients: HashMap::new(),
        })
    }

    fn run_once(&mut self, timeout: usize) -> nix::Result<()> {
        let mut evs: [KEvent; 64] = unsafe { mem::uninitialized() };
        let n = try!(kevent(self.kfd, &[], &mut evs, timeout));
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
                    try!(add_ev(self.kfd, fd, EventFilter::EVFILT_READ));
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
            try!(del_ev(self.kfd, cfd, EventFilter::EVFILT_READ));
            try!(add_ev(self.kfd, cfd, EventFilter::EVFILT_WRITE));
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
            try!(add_ev(self.kfd, cfd, EventFilter::EVFILT_READ));
            try!(del_ev(self.kfd, cfd, EventFilter::EVFILT_WRITE));
        }

        Ok(())
    }

    fn close_client(&mut self, cfd: RawFd) -> nix::Result<()> {
        try!(del_ev(self.kfd, cfd, EventFilter::EVFILT_READ));
        try!(del_ev(self.kfd, cfd, EventFilter::EVFILT_WRITE));

        self.clients.remove(&cfd);
        println!("[ TRACE ] Client {:?} closed", cfd);

        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = close(self.lfd);
        let _ = close(self.kfd);
    }
}

fn main() {
    let addr = SockAddr::new_inet(InetAddr::new(IpAddr::new_v4(127, 0, 0, 1), 3000));
    let mut server = Server::new(addr).unwrap();

    println!("[ TRACE ] Server running ...");
    while let Ok(..) = server.run_once(0) {}

}
