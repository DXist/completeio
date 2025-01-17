use std::{io, net::Shutdown};

use socket2::{Domain, Protocol, SockAddr, Socket as Socket2, Type};

use crate::impl_raw_fd;
#[cfg(feature = "runtime")]
use crate::{
    buf::{IntoInner, IoBuf, IoBufMut, VectoredBufWrapper},
    buf_try,
    driver::Fd,
    op::{
        Accept, Connect, Recv, RecvFrom, RecvFromVectored, RecvResultExt, RecvVectored, Send,
        SendTo, SendToVectored, SendVectored, UpdateBufferLen,
    },
    task::RUNTIME,
    Attacher, BufResult,
};

pub struct Socket {
    socket: Socket2,
    #[cfg(feature = "runtime")]
    attacher: Attacher,
}

impl Socket {
    pub fn from_socket2(socket: Socket2) -> Self {
        Self {
            socket,
            #[cfg(feature = "runtime")]
            attacher: Attacher::new(),
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn attach(&self) -> io::Result<Fd> {
        self.attacher.attach(self)
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            socket: self.socket.try_clone()?,
            #[cfg(feature = "runtime")]
            attacher: self.attacher.clone(),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SockAddr> {
        self.socket.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SockAddr> {
        self.socket.local_addr()
    }

    pub fn new(domain: Domain, ty: Type, protocol: Option<Protocol>) -> io::Result<Self> {
        let socket = Socket2::new(domain, ty, protocol)?;
        // On Linux we use blocking socket
        // Newer kernels have the patch that allows to arm io_uring poll mechanism for
        // non blocking socket when there is no connections in listen queue
        //
        // https://patchwork.kernel.org/project/linux-block/patch/f999615b-205c-49b7-b272-c4e42e45e09d@kernel.dk/#22949861
        #[cfg(all(unix, not(target_os = "linux")))]
        socket.set_nonblocking(true)?;
        Ok(Self::from_socket2(socket))
    }

    pub fn bind(addr: &SockAddr, ty: Type, protocol: Option<Protocol>) -> io::Result<Self> {
        let socket = Self::new(addr.domain(), ty, protocol)?;
        socket.socket.bind(addr)?;
        Ok(socket)
    }

    pub fn listen(&self, backlog: i32) -> io::Result<()> {
        self.socket.listen(backlog)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.socket.shutdown(how)
    }

    pub fn connect(&self, addr: &SockAddr) -> io::Result<()> {
        self.socket.connect(addr)
    }

    #[cfg(feature = "runtime")]
    pub async fn connect_async(&self, addr: &SockAddr) -> io::Result<()> {
        let fd = self.attach()?;
        let op = Connect::new(fd, addr.clone());
        let (res, op) = RUNTIME.with(|runtime| runtime.submit(op)).await;
        op.on_connect(res)
    }

    #[cfg(feature = "runtime")]
    pub async fn accept(&self) -> io::Result<(Self, SockAddr)> {
        let fd = self.attach()?;
        #[cfg(unix)]
        let op = Accept::new(fd);
        #[cfg(target_os = "windows")]
        let op = {
            let local_addr = self.local_addr()?;
            Accept::with_socket_opts(
                fd,
                local_addr.domain(),
                self.socket.r#type()?,
                self.socket.protocol()?,
            )
        };
        let (res, mut op) = RUNTIME.with(|runtime| runtime.submit(op)).await;
        let (accept_sock, addr) = op.on_accept(res)?;
        Ok((Self::from_socket2(accept_sock), addr.clone()))
    }

    #[cfg(feature = "runtime")]
    pub async fn recv<T: IoBufMut<'static>>(&self, buffer: T) -> BufResult<usize, T> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = Recv::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
            .update_buffer_len()
    }

    #[cfg(feature = "runtime")]
    pub async fn recv_exact<T: IoBufMut<'static>>(&self, mut buffer: T) -> BufResult<usize, T> {
        let need = buffer.as_uninit_slice().len();
        let mut total_read = 0;
        let mut read;
        while total_read < need {
            (read, buffer) = buf_try!(self.recv(buffer).await);
            total_read += read;
        }
        let res = if total_read < need {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            ))
        } else {
            Ok(total_read)
        };
        (res, buffer)
    }

    #[cfg(feature = "runtime")]
    pub async fn recv_vectored<T: IoBufMut<'static>>(
        &self,
        buffer: VectoredBufWrapper<'static, T>,
    ) -> BufResult<usize, VectoredBufWrapper<'static, T>> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = RecvVectored::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
            .update_buffer_len()
    }

    #[cfg(feature = "runtime")]
    pub async fn send<T: IoBuf<'static>>(&self, buffer: T) -> BufResult<usize, T> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = Send::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
    }

    #[cfg(feature = "runtime")]
    pub async fn send_all<T: IoBuf<'static>>(&self, mut buffer: T) -> BufResult<usize, T> {
        let buf_len = buffer.buf_len();
        let mut total_written = 0;
        let mut written;
        while total_written < buf_len {
            (written, buffer) =
                buf_try!(self.send(buffer.slice(total_written..)).await.into_inner());
            total_written += written;
        }
        (Ok(total_written), buffer)
    }

    #[cfg(feature = "runtime")]
    pub async fn send_vectored<T: IoBuf<'static>>(
        &self,
        buffer: VectoredBufWrapper<'static, T>,
    ) -> BufResult<usize, VectoredBufWrapper<'static, T>> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = SendVectored::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
    }

    #[cfg(feature = "runtime")]
    pub async fn recv_from<T: IoBufMut<'static>>(
        &self,
        buffer: T,
    ) -> BufResult<(usize, SockAddr), T> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = RecvFrom::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
            .map_addr()
            .update_buffer_len()
    }

    #[cfg(feature = "runtime")]
    pub async fn recv_from_vectored<T: IoBufMut<'static>>(
        &self,
        buffer: VectoredBufWrapper<'static, T>,
    ) -> BufResult<(usize, SockAddr), VectoredBufWrapper<'static, T>> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = RecvFromVectored::new(fd, buffer);
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
            .map_addr()
            .update_buffer_len()
    }

    #[cfg(feature = "runtime")]
    pub async fn send_to<T: IoBuf<'static>>(
        &self,
        buffer: T,
        addr: &SockAddr,
    ) -> BufResult<usize, T> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = SendTo::new(fd, buffer, addr.clone());
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
    }

    #[cfg(feature = "runtime")]
    pub async fn send_to_vectored<T: IoBuf<'static>>(
        &self,
        buffer: VectoredBufWrapper<'static, T>,
        addr: &SockAddr,
    ) -> BufResult<usize, VectoredBufWrapper<'static, T>> {
        let (fd, buffer) = buf_try!(self.attach(), buffer);
        let op = SendToVectored::new(fd, buffer, addr.clone());
        RUNTIME
            .with(|runtime| runtime.submit(op))
            .await
            .into_inner()
    }
}

impl_raw_fd!(Socket, socket, attacher);
