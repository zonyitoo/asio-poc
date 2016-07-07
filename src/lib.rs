extern crate nix;

#[cfg_attr(rustfmt, rustfmt_skip)]
#[cfg(any(
    target_os = "linux",
    target_os = "android",
))]
mod epoll;

#[cfg_attr(rustfmt, rustfmt_skip)]
#[cfg(any(
    target_os = "linux",
    target_os = "android",
))]
pub use self::epoll::{Reactor, DescriptorState};

#[cfg_attr(rustfmt, rustfmt_skip)]
#[cfg(any(
    target_os = "bitrig",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
))]
mod kqueue;

#[cfg_attr(rustfmt, rustfmt_skip)]
#[cfg(any(
    target_os = "bitrig",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "ios",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
))]
pub use self::kqueue::{Reactor, DescriptorState};

mod io_context;
mod net;
mod operation;

pub use io_context::IoContext;
