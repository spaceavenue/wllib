//! File-descriptor write helpers.

use rustix::fd::AsFd;
use rustix::io::{Errno, write};
use rustix::stdio;

use crate::error::SysError;

/// Writes `msg` to the file descriptor. Retries on partial write and `EINTR` until all bytes have
/// been written or a real error occurs.
///
/// `count` optionally caps how many bytes are written. `None` implies writing all bytes.
pub fn write_fd<Fd: AsFd>(
  fd: Fd,
  msg: impl AsRef<[u8]>,
  count: Option<usize>,
) -> Result<(), SysError> {
  let msg = msg.as_ref();
  let len = match count {
    Some(c) => c.min(msg.len()),
    None => msg.len(),
  };

  // truncate once up front so every iteration below can just trust `msg.len()`
  let mut msg = &msg[..len];
  while !msg.is_empty() {
    let res = write(fd.as_fd(), msg).map_err(|e| SysError::from_errno("write", e))?;
    match res {
      0 => break,
      res if res > 0 => {
        // the kernel accepted fewer bytes than asked, retry
        msg = &msg[res..];
      }
      _ => {
        let err = SysError::last("write");
        if err.errno == Errno::INTR.raw_os_error() {
          continue;
        }
        return Err(err);
      }
    }
  }
  Ok(())
}

/// Writes the buffer to stderr.
pub fn write_stderr(msg: impl AsRef<[u8]>) {
  let fd = unsafe { stdio::stderr() };
  let _ = write_fd(fd, msg, None);
}

/// Writes the buffer to stdout.
pub fn write_stdout(msg: impl AsRef<[u8]>) {
  let fd = unsafe { stdio::stdout() };
  let _ = write_fd(fd, msg, None);
}
