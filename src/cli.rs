//! `getopt_long` bindings.

#[repr(C)]
pub struct LongOption {
  pub name: *const libc::c_char,
  pub has_arg: libc::c_int,
  pub flag: *mut libc::c_int,
  pub val: libc::c_int,
}
impl LongOption {
  #[must_use]
  pub const fn new(name: &core::ffi::CStr, has_arg: libc::c_int, val: char) -> Self {
    Self {
      name: name.as_ptr().cast(),
      has_arg,
      flag: core::ptr::null_mut(),
      val: val as _,
    }
  }
}

pub const NO_ARGUMENT: libc::c_int = 0;
pub const REQUIRED_ARGUMENT: libc::c_int = 1;
pub const OPTIONAL_ARGUMENT: libc::c_int = 2;

/// Every `LongOption` array passed to `getopt_long` must end with an all-zero entry marking where
/// the array ends, since the array itself carries no separate length.
pub const LONG_OPTION_TERMINATOR: LongOption = LongOption {
  name: core::ptr::null(),
  has_arg: 0,
  flag: core::ptr::null_mut(),
  val: 0,
};

unsafe extern "C" {
  pub fn getopt_long(
    argc: libc::c_int,
    argv: *const *mut libc::c_char,
    optstring: *const libc::c_char,
    longopts: *const LongOption,
    longindex: *mut libc::c_int,
  ) -> libc::c_int;
}
