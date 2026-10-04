//! The bounds an enrolment, an assignment and a capability report are declared
//! with, and the two `custom` rules garde has no built-in for.
//!
//! The caps are `register.zig`'s, `protocol_policy.zig`'s and
//! `protocol_bind.zig`'s, so a runner and either daemon agree on what is legal.
//! The rules judge content only; each field's length is garde's own `length`,
//! declared beside the rule.

use afd_validate::{ascii_digits, charset, nul_free};

/// The longest host identifier an enrolment may name.
pub const HOST_ID_MAX_BYTES: usize = 256;

/// The most labels one enrolment may carry. Labels are stored on the runner
/// row and re-read by every runner page, so an unbounded list is a
/// persistence-amplification channel.
pub const LABELS_MAX: usize = 32;

/// The longest one label may be.
pub const LABEL_MAX_BYTES: usize = 64;

/// The most registry hosts one assignment may name.
pub const REGISTRY_ENTRIES_MAX: usize = 32;

/// The longest registry entry: a 253-character host, a colon, a five-digit
/// port.
pub const REGISTRY_ENTRY_MAX_BYTES: usize = 259;

/// The longest decimal port a registry entry may carry.
pub const REGISTRY_PORT_MAX_DIGITS: usize = 5;

/// The most extra binds one assignment may add.
pub const EXTRA_BINDS_MAX: usize = 16;

/// The shortest bind path: `/` and one character.
pub const BIND_PATH_MIN_BYTES: usize = 2;

/// The longest bind path.
pub const BIND_PATH_MAX_BYTES: usize = 4096;

/// The longest operator note on one bind.
pub const BIND_NOTE_MAX_BYTES: usize = 200;

/// The most cgroup controllers one capability report may name.
pub const REPORT_CONTROLLERS_MAX: usize = 16;

/// The longest one controller name may be.
pub const CONTROLLER_NAME_MAX_BYTES: usize = 64;

/// What [`registry_entry`] reports.
const NOT_HOST_PORT: &str = "must be a bare host or host:port";

/// What [`bind_path`] reports.
const NOT_SAFE_BIND: &str =
    "must be an absolute canonical path outside the protected and sensitive sets";

/// Separates a registry host from its port.
const PORT_SEPARATOR: char = ':';

/// Separates path segments.
const SEGMENT_SEPARATOR: char = '/';

/// Path segments that name a directory relative to another.
const RELATIVE_SEGMENTS: [&str; 2] = [".", ".."];

/// Every daemon-owned or sensitive subtree an operator bind must not overlap.
///
/// The union of `BASELINE_RO_PATHS` and `SENSITIVE_PATHS` in
/// `protocol_bind_paths.zig`, kept as two lists for the reason each exists.
const PROTECTED_BIND_PATHS: [&str; 14] = [
    "/etc/ssl/certs",
    "/run/systemd/resolve",
    "/etc/hosts",
    "/etc/nsswitch.conf",
    "/usr",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/proc",
    "/dev",
    "/tmp",
    "/root",
    "/home",
];

/// The subtrees a bind must not reach because they hold host or daemon state.
const SENSITIVE_BIND_PATHS: [&str; 7] = [
    "/boot",
    "/sys",
    "/run",
    "/var/run",
    "/var/lib/agentsfleet",
    "/opt/agentsfleet",
    "/etc",
];

/// A registry entry is a bare `host` or `host:port` name.
///
/// Deliberately NOT a URL: a scheme, a path or a space is refused, because the
/// value becomes an egress allowlist entry and a permissive parse there is a
/// hole in the cage. A second colon lands in the port and fails its digits,
/// as the Zig's `indexOfScalar` split does.
///
/// # Errors
/// [`NOT_HOST_PORT`] for an empty host, a host outside `[A-Za-z0-9_.-]`, or a
/// port that is not one to five digits.
pub(super) fn registry_entry<C: ?Sized>(entry: &str, context: &C) -> garde::Result {
    let (host, port) = entry
        .split_once(PORT_SEPARATOR)
        .map_or((entry, None), |(host, port)| (host, Some(port)));
    let host_ok = !host.is_empty() && charset(is_registry_host_char)(host, context).is_ok();
    let port_ok = port.is_none_or(|port| {
        (1..=REGISTRY_PORT_MAX_DIGITS).contains(&port.len()) && ascii_digits(port, context).is_ok()
    });
    if host_ok && port_ok {
        Ok(())
    } else {
        Err(garde::Error::new(NOT_HOST_PORT))
    }
}

/// A host character a registry entry may carry.
const fn is_registry_host_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
}

/// A bind path is absolute, canonical, NUL-free, and clear of every protected
/// subtree.
///
/// Canonical means no trailing `/`, no empty segment and no `.` or `..`, so
/// the overlap test below compares the path the sandbox will actually mount.
///
/// # Errors
/// [`NOT_SAFE_BIND`] for a relative, non-canonical or overlapping path.
pub(super) fn bind_path<C: ?Sized>(path: &str, context: &C) -> garde::Result {
    let canonical = path.strip_prefix(SEGMENT_SEPARATOR).is_some_and(|rest| {
        rest.split(SEGMENT_SEPARATOR)
            .all(|segment| !segment.is_empty() && !RELATIVE_SEGMENTS.contains(&segment))
    });
    let clear = PROTECTED_BIND_PATHS
        .iter()
        .chain(SENSITIVE_BIND_PATHS.iter())
        .all(|protected| !paths_overlap(path, protected));
    if canonical && clear && nul_free(path, context).is_ok() {
        Ok(())
    } else {
        Err(garde::Error::new(NOT_SAFE_BIND))
    }
}

/// Whether either path contains the other, segment-aware: `/etcetera` does not
/// overlap `/etc`.
fn paths_overlap(left: &str, right: &str) -> bool {
    left == right || contains_path(left, right) || contains_path(right, left)
}

/// Whether `child` sits below `parent`.
fn contains_path(parent: &str, child: &str) -> bool {
    child
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with(SEGMENT_SEPARATOR))
}
