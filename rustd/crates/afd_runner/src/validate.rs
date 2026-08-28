//! What an enrolment request must satisfy before a row is written.
//!
//! Parse, don't validate: each function answers a `Result`, so a caller that
//! reached the write has a value the write can trust and needs no defensive
//! re-check (`dispatch/write_rust.md` §Functional design). The bounds and the
//! refusal sentences are `protocol_policy.zig`'s and `register.zig`'s, pinned
//! byte-for-byte — a client reads them.

use afd_core::limits::WorkerCount;
use afd_wire::runner::{AssignedPolicy, ExtraBind};

use crate::error::{DETAIL_HOST_ID_BOUNDS, DETAIL_REGISTRY_ALLOWLIST, Result, rejected};

/// `protocol_bind.zig`'s refusal for an unsafe operator-added mount.
pub const DETAIL_EXTRA_BINDS: &str = "extra_binds entries must be absolute host paths outside the daemon-owned baseline and the sensitive set, with no traversal";

/// `register.zig`'s `MAX_HOST_ID_LEN`.
const MAX_HOST_ID_LEN: usize = 256;

/// `protocol_policy.zig`'s `MAX_REGISTRY_ENTRIES`.
const MAX_REGISTRY_ENTRIES: usize = 32;

/// `protocol_policy.zig`'s `MAX_REGISTRY_HOST_LEN` — a 253-character host, a
/// colon, and a five-digit port.
const MAX_REGISTRY_HOST_LEN: usize = 259;

/// Longest decimal port a registry entry may carry.
const MAX_PORT_DIGITS: usize = 5;

const MAX_EXTRA_BINDS: usize = 16;
const MAX_BIND_PATH_LEN: usize = 4096;
const MAX_BIND_NOTE_LEN: usize = 200;

/// Every daemon-owned or sensitive subtree an operator bind must not overlap.
///
/// This is the union of `BASELINE_RO_PATHS` and `SENSITIVE_PATHS` in
/// `protocol_bind_paths.zig`. Keeping the union removes harmless duplicates
/// while preserving the same segment-aware boundary.
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

const SENSITIVE_BIND_PATHS: [&str; 7] = [
    "/boot",
    "/sys",
    "/run",
    "/var/run",
    "/var/lib/agentsfleet",
    "/opt/agentsfleet",
    "/etc",
];

/// The host identifier an enrolment names, once it is known to be usable.
///
/// A newtype whose constructor is the only way in, so the length rule is
/// checked once at the boundary and every later use is a value that already
/// satisfies it (`M-STRONG-TYPES-GUARD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostId<'a>(&'a str);

impl<'a> HostId<'a> {
    /// Accepts a host identifier of a usable length.
    ///
    /// # Errors
    /// Refuses an empty identifier, or one past [`MAX_HOST_ID_LEN`], quoting
    /// `register.zig`'s sentence.
    pub fn new(raw: &'a str) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_HOST_ID_LEN {
            return Err(rejected(DETAIL_HOST_ID_BOUNDS));
        }
        Ok(Self(raw))
    }

    /// The identifier, for the bind and the enrolment event's metadata.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// Whether one allowlist entry is a bare `host` or `host:port` name.
///
/// Deliberately NOT a URL: a scheme, a path or a space is refused, because the
/// value becomes an egress allowlist entry and a permissive parse there is a
/// hole in the cage.
fn registry_host_valid(entry: &str) -> bool {
    if entry.is_empty() || entry.len() > MAX_REGISTRY_HOST_LEN {
        return false;
    }
    let (host, port) = match entry.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (entry, None),
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
    {
        return false;
    }
    // `split_once` splits on the FIRST colon, so a second one lands in `port`
    // and fails the digit test below — which is the Zig behaviour, where
    // `indexOfScalar` finds the first and the remainder must be all digits.
    match port {
        None => true,
        Some(port) => {
            !port.is_empty()
                && port.len() <= MAX_PORT_DIGITS
                && port.bytes().all(|c| c.is_ascii_digit())
        }
    }
}

/// The assignment as it will be STORED, with the worker count clamped.
///
/// Returned rather than mutated in place so the caller cannot forget to use it:
/// what is echoed to the enrolling operator must be what the host will apply,
/// and a clamp written back into the request would leave two values in scope
/// with only a comment saying which is authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredAssignment {
    /// The clamped worker ceiling.
    pub worker_count: WorkerCount,
}

/// Checks an assignment and resolves what will actually be stored.
///
/// # Errors
/// Refuses an allowlist that is too long or carries an entry that is not a
/// `host[:port]` name, quoting `register.zig`'s sentence.
pub fn assignment(policy: &AssignedPolicy<'_>) -> Result<StoredAssignment> {
    if policy.registry_allowlist.len() > MAX_REGISTRY_ENTRIES
        || !policy
            .registry_allowlist
            .iter()
            .all(|entry| registry_host_valid(entry))
    {
        return Err(rejected(DETAIL_REGISTRY_ALLOWLIST));
    }
    if !extra_binds_valid(&policy.extra_binds) {
        return Err(rejected(DETAIL_EXTRA_BINDS));
    }
    // Clamped, never refused: `register.zig` clamps into the shared bounds so
    // what is echoed is what runs, and `WorkerCount::clamping` is the same
    // rule already expressed as a type.
    Ok(StoredAssignment {
        worker_count: WorkerCount::clamping(policy.worker_count),
    })
}

fn extra_binds_valid(binds: &[ExtraBind<'_>]) -> bool {
    binds.len() <= MAX_EXTRA_BINDS
        && binds.iter().all(|bind| {
            bind.note.len() <= MAX_BIND_NOTE_LEN
                && bind_path_valid(&bind.path)
                && PROTECTED_BIND_PATHS
                    .iter()
                    .chain(SENSITIVE_BIND_PATHS.iter())
                    .all(|protected| !paths_overlap(&bind.path, protected))
        })
}

fn bind_path_valid(path: &str) -> bool {
    (2..=MAX_BIND_PATH_LEN).contains(&path.len())
        && path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains('\0')
        && path[1..]
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right || contains_path(left, right) || contains_path(right, left)
}

fn contains_path(parent: &str, child: &str) -> bool {
    child
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
mod extra_bind_tests {
    use std::borrow::Cow;

    use afd_wire::runner::BindMode;

    use super::*;

    fn bind(path: &str) -> ExtraBind<'_> {
        ExtraBind {
            path: Cow::Borrowed(path),
            mode: BindMode::ReadOnly,
            note: Cow::Borrowed("operator reason"),
        }
    }

    #[test]
    fn test_extra_bind_validation_accepts_only_canonical_unprotected_paths() {
        assert!(extra_binds_valid(&[bind("/srv/models")]));
        for refused in [
            "relative/path",
            "/srv/../root",
            "/srv/data/",
            "/",
            "/etc/ssl",
            "/run",
            "/var",
            "/etc/./ssl",
            "//etc",
        ] {
            assert!(!extra_binds_valid(&[bind(refused)]), "accepted {refused}");
        }
        assert!(extra_binds_valid(&[bind("/etcetera")]));
    }

    #[test]
    fn test_extra_bind_validation_enforces_list_path_and_note_bounds() {
        let at_cap = (0..MAX_EXTRA_BINDS)
            .map(|index| ExtraBind {
                path: Cow::Owned(format!("/srv/models-{index}")),
                mode: BindMode::ReadOnly,
                note: Cow::Borrowed(""),
            })
            .collect::<Vec<_>>();
        assert!(extra_binds_valid(&at_cap));

        let mut over = at_cap;
        over.push(bind("/srv/one-too-many"));
        assert!(!extra_binds_valid(&over));
        assert!(!extra_binds_valid(&[ExtraBind {
            note: Cow::Owned("n".repeat(MAX_BIND_NOTE_LEN + 1)),
            ..bind("/srv/models")
        }]));
        assert!(!extra_binds_valid(&[bind(&format!(
            "/{}",
            "a".repeat(MAX_BIND_PATH_LEN)
        ))]));
    }
}
