use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

use arrayvec::ArrayVec;

use crate::{impl_raw_fd, op::Read, syscall, task::RUNTIME};

/// An event that won't wake until [`EventHandle::notify`] is called
/// successfully.
#[derive(Debug)]
pub struct Event {
    fd: OwnedFd,
}

impl Event {
    /// Create [`Event`].
    pub fn new() -> io::Result<Self> {
        let fd = syscall!(eventfd(0, 0))?;
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(Self { fd })
    }

    /// Get a notify handle.
    pub fn handle(&self) -> io::Result<EventHandle> {
        Ok(EventHandle::new(self.fd.try_clone()?))
    }

    /// Wait for [`EventHandle::notify`] called.
    pub async fn wait(&self) -> io::Result<()> {
        let buffer = ArrayVec::<u8, 8>::new();
        let fd = RUNTIME.with(|runtime| runtime.attach(self.fd.as_raw_fd()))?;
        let op = Read::new(fd, buffer);
        let (res, _) = RUNTIME.with(|runtime| runtime.submit(op)).await;
        res?;
        Ok(())
    }
}

impl_raw_fd!(Event, fd);

/// A handle to [`Event`].
pub struct EventHandle {
    fd: OwnedFd,
}

impl EventHandle {
    pub(crate) fn new(fd: OwnedFd) -> Self {
        Self { fd }
    }

    /// Notify the event.
    pub fn notify(&mut self) -> io::Result<()> {
        let data = 1u64;
        syscall!(write(
            self.fd.as_raw_fd(),
            &data as *const _ as *const _,
            std::mem::size_of::<u64>(),
        ))?;
        Ok(())
    }
}

impl_raw_fd!(EventHandle, fd);
