use std::{
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
};

use crate::{impl_raw_fd, op::ReadAt, task::RUNTIME};

#[derive(Debug)]
pub struct Event {
    fd: OwnedFd,
}

impl Event {
    pub fn new() -> io::Result<Self> {
        let fd = unsafe { libc::eventfd(0, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(Self { fd })
    }

    pub fn handle(&self) -> EventHandle {
        EventHandle::new(self.fd.as_fd())
    }

    pub async fn wait(&self) -> io::Result<()> {
        let buffer = Vec::with_capacity(8);
        let op = ReadAt::new(self.as_raw_fd(), 0, buffer);
        let (res, _) = RUNTIME.with(|runtime| runtime.submit(op)).await;
        res?;
        Ok(())
    }
}

impl_raw_fd!(Event, fd);

pub struct EventHandle<'a> {
    fd: BorrowedFd<'a>,
}

impl<'a> EventHandle<'a> {
    pub(crate) fn new(fd: BorrowedFd<'a>) -> Self {
        Self { fd }
    }

    pub fn notify(&self) -> io::Result<()> {
        let data = 1u64;
        let res = unsafe {
            libc::write(
                self.fd.as_raw_fd(),
                &data as *const _ as *const _,
                std::mem::size_of::<u64>(),
            )
        };
        if res < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
