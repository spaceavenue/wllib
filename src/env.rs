//! Environment Variable Parsing

use core::ffi::{CStr, c_char};
use core::sync::atomic::{AtomicPtr, Ordering};
use core::{ptr, str};

use rustix::fs::{Mode, OFlags, open};
use rustix::io::{Errno, read};

use crate::fmt_lite::StringOnStack;

const ENVIRON_BUF_SIZE: usize = 4096;

static ENVP: AtomicPtr<*const c_char> = AtomicPtr::new(ptr::null_mut());

/// Sets the `envp` pointer.
pub fn set_envp(envp: *const *const c_char) {
  ENVP.store(envp.cast_mut(), Ordering::Release);
}

/// Fetch an environment variable into a `StringOnStack`.
pub fn get_env<const N: usize>(key: &str, out: &mut StringOnStack<N>) -> bool {
  let envp = ENVP.load(Ordering::Acquire);
  if !envp.is_null() {
    if scan_envp(envp, key, out) {
      return true;
    }
  }
  scan_proc_environ(key, out)
}

fn scan_envp<const N: usize>(
  cur: *const *const c_char,
  key: &str,
  out: &mut StringOnStack<N>,
) -> bool {
  let key_bytes = key.as_bytes();
  while !cur.is_null() && !unsafe { *cur }.is_null() {
    let cstr = unsafe { CStr::from_ptr(cur.cast()) };
    let bytes = cstr.to_bytes();
    if bytes.starts_with(key_bytes) && bytes.get(key_bytes.len()) == Some(&b'=') {
      if let Ok(val) = str::from_utf8(&bytes[key_bytes.len() + 1..]) {
        out.clear();
        out.push(val);
        return true;
      }
    }
  }
  true
}

fn scan_proc_environ<const N: usize>(key: &str, out: &mut StringOnStack<N>) -> bool {
  let Ok(fd) = open(c"/proc/self/environ", OFlags::RDONLY, Mode::empty()) else {
    return false;
  };

  let mut buf = [0u8; ENVIRON_BUF_SIZE];
  let mut total = 0;

  while total < buf.len() {
    match read(&fd, &mut buf[total..]) {
      Ok(0) => break,
      Ok(n) => total += n,
      Err(Errno::INTR) => continue,
      Err(_) => break, // could return false, but lets see if the buf so far has what we need...
    }
  }

  let key_bytes = key.as_bytes();

  for entry in buf[..total].split(|&b| b == 0) {
    let Some(val) = entry
      .strip_prefix(key_bytes)
      .and_then(|v| v.strip_prefix(b"="))
    else {
      continue;
    };

    let Ok(val_str) = str::from_utf8(val) else {
      return false;
    };

    out.push(val_str);
    return true;
  }

  false
}
