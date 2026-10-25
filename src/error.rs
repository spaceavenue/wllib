//! Error types for the transport/protocol layer.

use crate::fmt_lite::StringOnStack;
use crate::wire::{read_str, read_u32};

/// A failed syscall, captured with `errno` at the point of failure.
///
/// `call` is a fixed `&'static str` naming the syscall (e.g. `"socket"`).
#[derive(Clone, Copy)]
pub struct SysError {
  pub call: &'static str,
  pub errno: i32,
}

impl SysError {
  #[must_use]
  pub fn from_errno(call: &'static str, err: rustix::io::Errno) -> Self {
    Self {
      call,
      errno: err.raw_os_error(),
    }
  }
}

pub trait SysResultExt<T> {
  fn syserr(self, call: &'static str) -> Result<T, WireError>;
}

impl<T> SysResultExt<T> for Result<T, rustix::io::Errno> {
  #[inline]
  fn syserr(self, call: &'static str) -> Result<T, WireError> {
    self.map_err(|e| {
      WireError::Sys(SysError {
        call,
        errno: e.raw_os_error(),
      })
    })
  }
}

/// Max length of a message body we'll keep for diagnostics. Longer messages are truncated rather
/// than allocated.
pub const MESSAGE_CAP: usize = 128;

/// A parsed `wl_display::error` event: which object misbehaved, the protocol-defined error code,
/// and the compositor's human-readable explanation.
#[derive(Clone, Copy)]
pub struct ProtocolError {
  pub object_id: u32,
  pub code: u32,
  pub message: StringOnStack<MESSAGE_CAP>,
}

impl ProtocolError {
  #[must_use]
  pub fn from(buf: &[u8], idx: usize) -> Self {
    let object_id = read_u32(buf, idx + 8);
    let code = read_u32(buf, idx + 12);
    let mut message: StringOnStack<MESSAGE_CAP> = StringOnStack::new();
    if let Some((text, _)) = read_str(buf, idx + 16) {
      message.push(text);
    }
    Self {
      object_id,
      code,
      message,
    }
  }
}

/// Errors that can occur in the transport/protocol layer.
#[derive(Clone, Copy)]
pub enum WireError {
  /// A syscall used by the transport failed.
  Sys(SysError),
  /// The compositor sent a `wl_display::error` event, a fatal protocol violation. The
  /// connection should be treated as dead once this is observed.
  Protocol(ProtocolError),
  /// The provided socket path was invalid or exceeded the maximum Unix socket path length.
  InvalidPath,
  /// The buffer was too small for framing messages.
  BufferTooSmall,
  /// The compositor closed the connection (`recv` returned EOF).
  ConnectionClosed,
}
