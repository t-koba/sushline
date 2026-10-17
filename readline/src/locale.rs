/// Owned locale used for locale-sensitive one-to-one character lowercase.
///
/// The handle never changes the process or thread locale. Targets without the
/// POSIX locale APIs return `None`, allowing callers to retain byte fallback.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
pub(crate) struct LocaleLowercase {
    locale: libc::locale_t,
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
impl LocaleLowercase {
    pub(crate) fn new(locale_name: &str) -> Option<Self> {
        use std::ffi::CString;
        let locale_name = CString::new(locale_name).ok()?;
        // An owned locale object keeps casing independent of process-global
        // setlocale state and does not change any thread's active locale.
        let locale = unsafe {
            libc::newlocale(
                libc::LC_CTYPE_MASK,
                locale_name.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        (!locale.is_null()).then_some(Self { locale })
    }

    pub(crate) fn lowercase(&self, character: char) -> char {
        use std::os::raw::c_uint;
        unsafe extern "C" {
            fn towlower_l(character: c_uint, locale: libc::locale_t) -> c_uint;
        }
        let lowered = unsafe { towlower_l(character as c_uint, self.locale) };
        char::from_u32(lowered).unwrap_or(character)
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
impl Drop for LocaleLowercase {
    fn drop(&mut self) {
        unsafe { libc::freelocale(self.locale) };
    }
}

// Targets without POSIX locale_t/towlower_l retain the existing byte-folding
// path; this is intentionally a narrow fallback, not a Unicode casefold claim.
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
pub(crate) struct LocaleLowercase;

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
impl LocaleLowercase {
    pub(crate) fn new(_locale_name: &str) -> Option<Self> {
        None
    }

    pub(crate) fn lowercase(&self, character: char) -> char {
        character
    }
}
