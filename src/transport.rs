//! Socket connection and id allocation.

use core::mem::MaybeUninit;

use rustix::cmsg_space;
use rustix::fd::{AsFd, AsRawFd, OwnedFd, RawFd};
use rustix::io::{IoSlice, IoSliceMut};
use rustix::net::{
  AddressFamily, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SendAncillaryBuffer,
  SendAncillaryMessage, SendFlags, SocketAddrUnix, SocketFlags, SocketType, connect, recv, recvmsg,
  sendmsg, socket_with,
};

use crate::env::get_env;
use crate::error::{SysError, SysResultExt, WireError};
use crate::fmt_lite::StringOnStack;
use crate::wire::{Message, complete_message_len};

/// Size of the leftover-message carry buffer `Connection` keeps for [`Connection::recv_framed`].
/// Must be at least as large as any buffer a caller passes to `recv_framed`; a leftover can
/// never exceed what was read into that buffer in the first place.
const RECV_CARRY_CAP: usize = 4096;

/// The default path/socket name to use in case `WAYLAND_DISPLAY` is unset.
const DEFAULT_SOCK: &str = "wayland-0";

pub struct Connection {
  socket_fd: OwnedFd,
  next_id: u32,
  carry: [u8; RECV_CARRY_CAP],
  carry_len: usize,
}

impl Connection {
  /// Connect to the Wayland socket.
  pub fn connect() -> Result<Self, WireError> {
    let mut display = StringOnStack::<64>::new();
    let mut runtime = StringOnStack::<64>::new();
    let mut path = StringOnStack::<128>::new();

    let found_display = get_env("WAYLAND_DISPLAY", &mut display);
    let found_runtime = get_env("XDG_RUNTIME_DIR", &mut runtime);
    let display_str = if found_display {
      &display
    } else {
      DEFAULT_SOCK
    };

    if display_str.starts_with("/") {
      path.push(display_str);
    } else {
      if !found_runtime {
        return Err(WireError::Environment);
      }
      path.push(runtime);
      if !&path.ends_with('/') {
        path.push("/");
      }
      path.push(display_str);
    }

    let addr = SocketAddrUnix::new(path.as_c_str()).map_err(|_| WireError::Environment)?;

    let socket_fd = socket_with(
      AddressFamily::UNIX,
      SocketType::STREAM,
      SocketFlags::CLOEXEC,
      None,
    )
    .syserr("socket")?;

    connect(&socket_fd, &addr).syserr("connect")?;

    Ok(Self {
      socket_fd,
      next_id: 4,
      carry: [0; RECV_CARRY_CAP],
      carry_len: 0,
    })
  }

  #[must_use]
  pub fn as_raw_fd(&self) -> RawFd {
    self.socket_fd.as_raw_fd()
  }

  /// Allocate a unique client-side object id sequentially.
  pub fn alloc_id(&mut self) -> u32 {
    let id = self.next_id;
    self.next_id += 1;
    id
  }

  /// Send a protocol message over the socket. Optionally transmit fd along with the message using
  /// unix domain socket ancillary `SCM_RIGHTS` data.
  pub fn send(&self, msg: &Message, fd: Option<&OwnedFd>) -> Result<(), WireError> {
    let msg_bytes = msg.as_bytes();
    let iov = [IoSlice::new(msg_bytes)];

    if let Some(fd) = fd {
      let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
      let mut cmsg = SendAncillaryBuffer::new(&mut space);
      let borrowed = fd.as_fd();
      let fds = [borrowed];

      cmsg.push(SendAncillaryMessage::ScmRights(&fds));

      sendmsg(&self.socket_fd, &iov, &mut cmsg, SendFlags::empty()).syserr("sendmsg")?;
    } else {
      let mut cmsg = SendAncillaryBuffer::default();
      sendmsg(&self.socket_fd, &iov, &mut cmsg, SendFlags::empty()).syserr("sendmsg")?;
    }

    Ok(())
  }

  /// Convenience wrapper around [`Self::send`] that logs failures via
  /// [`WireError::write_diagnostic`].
  pub fn send_logged(&self, msg: &Message, fd: Option<&OwnedFd>) {
    if let Err(e) = self.send(msg, fd) {
      e.write_diagnostic();
    }
  }

  /// Read protocol data into `buf` and return what was received.
  pub fn recv<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], WireError> {
    let (_, bytes) = recv(&self.socket_fd, &mut *buf, RecvFlags::empty()).syserr("recv")?;
    if bytes > 0 {
      Ok(&buf[..bytes])
    } else {
      Err(WireError::ConnectionClosed)
    }
  }

  /// Like [`Self::recv`], but only hands back *complete* wire messages.
  pub fn recv_framed<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a [u8], WireError> {
    if buf.len() < RECV_CARRY_CAP {
      // dont know what else to return here
      return Err(WireError::Environment);
    }

    let carry_len = self.carry_len;
    buf[..carry_len].copy_from_slice(&self.carry[..carry_len]);

    let (bytes, _) =
      recv(&self.socket_fd, &mut buf[carry_len..], RecvFlags::empty()).syserr("recv")?;

    if bytes == 0 {
      return Err(WireError::ConnectionClosed);
    }

    let total = carry_len + bytes;
    let complete_end = complete_message_len(buf, total);
    let leftover = total - complete_end;

    self.carry[..leftover].copy_from_slice(&buf[complete_end..total]);
    self.carry_len = leftover;

    Ok(&buf[..complete_end])
  }

  /// Read protocol data into `buf` and return what was received, along with the first fd if one was
  /// received.
  pub fn recv_with_fd<'a>(
    &self,
    buf: &'a mut [u8],
  ) -> (Result<&'a [u8], WireError>, Option<OwnedFd>) {
    let mut iov = [IoSliceMut::new(buf)];
    let mut space = [MaybeUninit::uninit(); cmsg_space!(ScmRights(1))];
    let mut cmsg = RecvAncillaryBuffer::new(&mut space);

    let res = recvmsg(&self.socket_fd, &mut iov, &mut cmsg, RecvFlags::empty());

    let mut received_fd = None;

    for msg in cmsg.drain() {
      if let RecvAncillaryMessage::ScmRights(mut fds) = msg {
        received_fd = fds.next();
      }
    }

    match res {
      Ok(msg) if msg.bytes > 0 => (Ok(&buf[..msg.bytes]), received_fd),
      Ok(_) => (Err(WireError::ConnectionClosed), received_fd),
      Err(e) => (
        Err(WireError::Sys(SysError::from_errno("recvmsg", e))),
        received_fd,
      ),
    }
  }
}
