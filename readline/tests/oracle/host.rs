//! The oracle suite's host baseline.
//!
//! The oracle cases compare Sushline against behavior observed on a GNU Bash
//! 5.3 host, so that host is a precondition of the suite rather than a
//! sibling check. A case that ran on another host would compare against a
//! baseline the compatibility claims do not cover, so the suite stops with one
//! message instead of reporting such a comparison as evidence. The Readline
//! the host carries is part of that baseline: the claims name the release the
//! evidence was measured on, so a host that carries another release is a host
//! the claims do not describe.

use std::io::Write;
use std::process::Command;
use std::sync::OnceLock;

use super::host_readline;

/// The documented oracle host version.
const REQUIRED_BASH_VERSION: (u32, u32) = (5, 3);

/// The host probe, computed once per test binary.
static BASELINE: OnceLock<()> = OnceLock::new();

/// The `major.minor` version the banner reports, if it reports one.
fn reported_version(banner: &str) -> Option<(u32, u32)> {
    let rest = banner.split_once("version ")?.1;
    let token: String = rest
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn probe() -> Result<String, String> {
    let output = Command::new("bash")
        .arg("--version")
        .output()
        .map_err(|error| format!("bash is not available on PATH: {error}"))?;
    let banner = String::from_utf8_lossy(&output.stdout);
    if !banner.contains("GNU bash") {
        return Err(format!("the oracle host must be GNU bash, found: {banner}"));
    }
    match reported_version(&banner) {
        Some(version) if version == REQUIRED_BASH_VERSION => {}
        _ => {
            return Err(format!(
                "the oracle host must be GNU Bash {}.{}, found: {banner}",
                REQUIRED_BASH_VERSION.0, REQUIRED_BASH_VERSION.1
            ));
        }
    }
    match host_readline::host_readline_version() {
        Some(version) if host_readline::is_documented(version) => Ok(format!(
            "the oracle host is GNU Bash {}.{} carrying Readline {version}",
            REQUIRED_BASH_VERSION.0, REQUIRED_BASH_VERSION.1
        )),
        Some(version) => Err(format!(
            "the oracle host must carry a Readline the compatibility document covers, found Readline {version}"
        )),
        None => Ok(format!(
            "the oracle host is GNU Bash {}.{}; its Readline release is not readable on this platform",
            REQUIRED_BASH_VERSION.0, REQUIRED_BASH_VERSION.1
        )),
    }
}

/// Stop the oracle suite unless the host is the documented GNU Bash 5.3.
///
/// The mismatch is reported once, on standard error, and the whole suite
/// stops, so no oracle case reports success against another host.
pub(super) fn require_oracle_host() {
    BASELINE.get_or_init(|| {
        let mut stderr = std::io::stderr();
        match probe() {
            Ok(baseline) => {
                let _ = writeln!(stderr, "{baseline}");
            }
            Err(message) => {
                let _ = writeln!(stderr, "{message}");
                let _ = stderr.flush();
                std::process::exit(101);
            }
        }
        let _ = stderr.flush();
    });
}

#[cfg(test)]
mod tests {
    use super::reported_version;

    #[test]
    fn reported_version_reads_the_major_and_minor_numbers() {
        assert_eq!(
            reported_version("GNU bash, version 5.3.0(1)-release (x86_64-pc-linux-gnu)"),
            Some((5, 3))
        );
        assert_eq!(
            reported_version("GNU bash, version 5.3.15(1)-release"),
            Some((5, 3))
        );
        assert_eq!(
            reported_version("GNU bash, version 5.2.21(1)-release"),
            Some((5, 2))
        );
    }

    #[test]
    fn reported_version_is_absent_without_a_version_banner() {
        assert_eq!(reported_version("GNU bash, version unknown"), None);
        assert_eq!(reported_version("GNU bash"), None);
        assert_eq!(reported_version("GNU bash, version 5"), None);
    }
}
