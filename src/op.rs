//! The async operations.
//! Types in this mod represents the low-level operations passed to kernel.
//! The operation itself doesn't perform anything.
//! You need to pass them to [`compio::driver::Driver`], and poll the driver.

use crate::{
    buf::{BufWrapper, IntoInner, IoBuf, IoBufMut, WrapBuf, WrapBufMut},
    driver::RawFd,
    BufResult,
};

pub(crate) trait BufResultExt {
    fn map_advanced(self) -> Self;
}

impl<T: WrapBufMut> BufResultExt for BufResult<usize, T> {
    fn map_advanced(self) -> Self {
        let (res, buffer) = self;
        let (res, buffer) = (res.map(|res| (res, ())), buffer).map_advanced();
        let res = res.map(|(res, _)| res);
        (res, buffer)
    }
}

impl<T: WrapBufMut, O> BufResultExt for BufResult<(usize, O), T> {
    fn map_advanced(self) -> Self {
        let (res, mut buffer) = self;
        if let Ok((init, _)) = &res {
            buffer.set_init(*init);
        }
        (res, buffer)
    }
}

/// Read a file at specified position into specified buffer.
#[derive(Debug)]
pub struct ReadAt<T: IoBufMut> {
    pub(crate) fd: RawFd,
    pub(crate) offset: usize,
    pub(crate) buffer: BufWrapper<T>,
}

impl<T: IoBufMut> ReadAt<T> {
    /// Create [`ReadAt`].
    pub fn new(fd: RawFd, offset: usize, buffer: T) -> Self {
        Self {
            fd,
            offset,
            buffer: BufWrapper::new(buffer),
        }
    }
}

impl<T: IoBufMut> IntoInner for ReadAt<T> {
    type Inner = BufWrapper<T>;

    fn into_inner(self) -> Self::Inner {
        self.buffer
    }
}

/// Write a file at specified position from specified buffer.
#[derive(Debug)]
pub struct WriteAt<T: IoBuf> {
    pub(crate) fd: RawFd,
    pub(crate) offset: usize,
    pub(crate) buffer: BufWrapper<T>,
}

impl<T: IoBuf> WriteAt<T> {
    /// Create [`WriteAt`].
    pub fn new(fd: RawFd, offset: usize, buffer: T) -> Self {
        Self {
            fd,
            offset,
            buffer: BufWrapper::new(buffer),
        }
    }
}

impl<T: IoBuf> IntoInner for WriteAt<T> {
    type Inner = BufWrapper<T>;

    fn into_inner(self) -> Self::Inner {
        self.buffer
    }
}