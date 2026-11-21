//! The bounds an enrolment, an assignment and a capability report are declared
//! with, the two `custom` rules garde has no built-in for, and the label-list
//! rule every writer of a runner's labels shares.
//!
//! The bounds are declared once here so `agentsfleet-runner` and
//! `agentsfleetd` agree on what is legal. The content rules judge content only;
//! each field's length is garde's own `length`, declared beside the rule.

use std::borrow::Cow;

use af_validate::{TEXT_MAX_CHARS, ascii_digits, charset, nul_free};
use garde::Validate as _;

/// The longest host identifier an enrolment may name: what one btree index
/// row holds.
///
/// The index is `runner.runners`' UNIQUE (`tenant_id`, `host_id`). Postgres
/// refuses an index row past 2,704 bytes; less its 8-byte tuple header, the
/// 16-byte `uuid` and the host id's 4-byte varlena header, 2,676 remain.
pub const HOST_ID_MAX_BYTES: usize = 2704 - 8 - 16 - 4;

/// The longest registry entry: a 253-character host, a colon, a five-digit
/// port.
pub const REGISTRY_ENTRY_MAX_BYTES: usize = 259;

/// The longest decimal port a registry entry may carry.
pub const REGISTRY_PORT_MAX_DIGITS: usize = 5;

/// The shortest bind path: `/` and one character.
pub const BIND_PATH_MIN_BYTES: usize = 2;

/// The longest bind path.
pub const BIND_PATH_MAX_BYTES: usize = 4096;

/// What [`label_list`] reports. Never read by a caller: each daemon route
/// answers the path with its own sentence.
const NOT_LABEL_LIST: &str = "must hold labels within the text ceiling";

/// A label list borrowed from whichever request carries it.
///
/// The one place the label bounds are declared. Enrolment and the operator's
/// label replacement both reach it through [`label_list`], so the two requests
/// cannot drift apart on what a legal label set is.
#[derive(garde::Validate)]
#[garde(transparent)]
struct LabelList<'r>(#[garde(inner(length(chars, max = TEXT_MAX_CHARS)))] &'r [Cow<'r, str>]);

/// Every label in a runner label list is at most [`TEXT_MAX_CHARS`]
/// characters.
///
/// # Errors
/// [`NOT_LABEL_LIST`], at the field's own path, for a list past either bound.
pub fn label_list<C: ?Sized>(labels: &[Cow<'_, str>], _context: &C) -> garde::Result {
    LabelList(labels)
        .validate()
        .map_err(|_report| garde::Error::new(NOT_LABEL_LIST))
}

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
/// Kept as two lists with `SENSITIVE_BIND_PATHS` for the reason each exists:
/// these are host paths an operator bind may not overlap, the other list holds
/// host and daemon state. [`bind_path`] refuses either overlap with one error.
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
/// hole in the cage. A second colon lands in the port and fails its digits:
/// `split_once` cuts at the first colon only.
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
