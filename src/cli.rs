//! `getopt_long` bindings.

use core::ffi::{c_char, c_int};
use core::ptr;

#[repr(C)]
pub struct LongOption {
  pub name: *const c_char,
  pub has_arg: c_int,
  pub flag: *mut c_int,
  pub val: c_int,
}

impl LongOption {
  #[must_use]
  pub const fn new(name: &core::ffi::CStr, has_arg: c_int, val: char) -> Self {
    Self {
      name: name.as_ptr().cast(),
      has_arg,
      flag: ptr::null_mut(),
      val: val as _,
    }
  }
}

pub const NO_ARGUMENT: c_int = 0;
pub const REQUIRED_ARGUMENT: c_int = 1;
pub const OPTIONAL_ARGUMENT: c_int = 2;

/// Every `LongOption` array passed to `getopt_long` must end with an all-zero entry marking where
/// the array ends, since the array itself carries no separate length.
pub const LONG_OPTION_TERMINATOR: LongOption = LongOption {
  name: ptr::null(),
  has_arg: 0,
  flag: ptr::null_mut(),
  val: 0,
};

unsafe extern "C" {
  pub fn getopt_long(
    argc: c_int,
    argv: *const *mut c_char,
    optstring: *const c_char,
    longopts: *const LongOption,
    longindex: *mut c_int,
  ) -> c_int;
}
