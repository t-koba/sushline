//! Private home-source selection shared by tilde consumers.
use std::ffi::{OsStr, OsString};

pub(crate) fn resolve(
    home: Option<&OsStr>,
    user: Option<&[u8]>,
    lookup: impl FnOnce(Option<&[u8]>) -> Option<OsString>,
) -> Option<OsString> {
    match (user, home) {
        (None, Some(home)) => Some(home.to_os_string()),
        _ => lookup(user),
    }
}

// None selects the current UID; a name selects the NSS named-account lookup.
#[cfg(unix)]
pub(crate) fn passwd_home_dir(user: Option<&[u8]>) -> Option<OsString> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStringExt;

    let name = user.map(CString::new).transpose().ok()?;
    // sysconf supplies a starting size, not a limit. If it has no suggestion,
    // start with the smallest buffer and let ERANGE drive growth.
    let mut size = usize::try_from(unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) })
        .ok()
        .filter(|size| *size > 0)
        .unwrap_or(1);
    let mut buffer = Vec::<u8>::new();
    loop {
        buffer.try_reserve(size.checked_sub(buffer.len())?).ok()?;
        buffer.resize(size, 0);
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: entry and result are writable, and buffer has `size` writable
        // bytes. The optional name is NUL-terminated and lives through the call.
        let status = unsafe {
            if let Some(name) = &name {
                libc::getpwnam_r(
                    name.as_ptr(),
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            } else {
                libc::getpwuid_r(
                    libc::getuid(),
                    entry.as_mut_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &mut result,
                )
            }
        };
        if status == libc::ERANGE {
            size = size.checked_mul(2)?;
            continue;
        }
        if status != 0 || result.is_null() {
            return None;
        }
        // SAFETY: a successful lookup with a non-null result initialized entry.
        // pw_dir is a NUL-terminated string in buffer; copy it before buffer drops.
        let home = unsafe { entry.assume_init().pw_dir };
        if home.is_null() {
            return None;
        }
        return Some(OsString::from_vec(unsafe {
            CStr::from_ptr(home).to_bytes().to_vec()
        }));
    }
}

#[cfg(not(unix))]
pub(crate) fn passwd_home_dir(_user: Option<&[u8]>) -> Option<OsString> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_home_selection_preserves_present_home_even_when_empty() {
        for home in ["", "/environment-home"] {
            assert_eq!(
                resolve(Some(OsStr::new(home)), None, |_| panic!("HOME is present")),
                Some(OsString::from(home)),
            );
        }
    }

    #[test]
    fn tilde_home_selection_uses_current_uid_or_named_lookup() {
        for (home, user) in [
            (None, None),
            (
                Some(OsStr::new("/ignored")),
                Some(b"synthetic-user".as_slice()),
            ),
        ] {
            assert_eq!(
                resolve(home, user, |requested| {
                    assert_eq!(requested, user);
                    Some(OsString::from("/synthetic-nss-home"))
                }),
                Some(OsString::from("/synthetic-nss-home")),
            );
            assert_eq!(resolve(home, user, |_| None), None);
        }
    }
}
