//! The shape rules garde calls back into.
//!
//! Every function here is a `custom` predicate on a field of the schema, and
//! every one of them takes `&String` and `&()` because that is garde's
//! callback contract — not a choice this crate made. Both lints below fire on
//! that signature and would keep firing however the bodies were written, so
//! the expectation is stated ONCE for the module rather than copied onto each
//! function: this file exists for nothing else, so a module-wide statement is
//! exactly as narrow as three per-function ones and cannot fall out of step
//! with them.

#![expect(
    clippy::ptr_arg,
    clippy::trivially_copy_pass_by_ref,
    reason = "these signatures are garde's `custom` callback contract, not this crate's choice"
)]

use super::{REASON_NOT_REPOSITORY, REASON_WHITESPACE};

/// Refuses an entry carrying whitespace.
///
/// Every one of these is a header name, host, path, event or tool that reaches
/// a comparison or a command line, where an embedded space is either a silent
/// mismatch or a second argument.
pub(super) fn is_token(entry: &String, (): &()) -> garde::Result {
    if carries_whitespace(entry) {
        return Err(garde::Error::new(REASON_WHITESPACE));
    }
    Ok(())
}

/// Refuses an entry that is not an `owner/name` repository.
///
/// Exactly one separator with a non-empty side on each — `owner/`, `/name` and
/// `owner/name/extra` are all refused.
pub(super) fn is_repository(entry: &String, (): &()) -> garde::Result {
    match entry.split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            if carries_whitespace(entry) {
                return Err(garde::Error::new(REASON_WHITESPACE));
            }
            Ok(())
        }
        _ => Err(garde::Error::new(REASON_NOT_REPOSITORY)),
    }
}

/// Whether `entry` carries whitespace anywhere.
///
/// A `&str` helper so both predicates above have bodies that would work with a
/// slice; the `&String` in their signatures is garde's contract alone.
fn carries_whitespace(entry: &str) -> bool {
    entry.contains(char::is_whitespace)
}

/// Bytes a git ref may never contain, beyond the control range.
const FORBIDDEN_REF_BYTES: &str = "~^:?*[\\";
/// Sequences a git ref may never contain.
const FORBIDDEN_REF_SEQUENCES: [&str; 3] = ["..", "//", "@{"];
/// The suffix a git ref may never end with.
const FORBIDDEN_REF_SUFFIX: &str = ".lock";
/// Why a base branch was refused.
const REASON_NOT_BRANCH: &str = "it is not a git branch name";

/// Refuses a base branch that could reach outside itself.
///
/// git's own rules for a ref component, kept because the value reaches a git
/// command line: a name that slips a `..` or an `@{` past here is a reference
/// to something other than the branch an operator authorised.
pub(super) fn is_branch_name(base: &String, (): &()) -> garde::Result {
    let ends_cleanly = !base.starts_with('/')
        && !base.ends_with('/')
        && !base.ends_with('.')
        && !base.ends_with(FORBIDDEN_REF_SUFFIX);
    let no_sequences = !FORBIDDEN_REF_SEQUENCES
        .iter()
        .any(|forbidden| base.contains(forbidden));
    let bytes_allowed = base
        .bytes()
        .all(|byte| byte > 0x20 && byte != 0x7f && !FORBIDDEN_REF_BYTES.contains(byte as char));

    if ends_cleanly && no_sequences && bytes_allowed {
        return Ok(());
    }
    Err(garde::Error::new(REASON_NOT_BRANCH))
}

/// The gate kinds only the daemon may raise.
///
/// Spelled here rather than imported because this crate sits UNDER the two that
/// own them — `afd_gate` holds `KIND_REPOSITORY_WRITE` and `afd_approval` holds
/// `KIND_INTEGRATION_GRANT`, and both depend on this one. Each owner pins its
/// constant against this list in its own test, which is the same shape the
/// repository already uses where a literal cannot be imported.
pub(super) const DAEMON_OWNED_GATE_KINDS: [&str; 2] = ["integration_grant", "repository_write"];

/// Refuses a gate kind the daemon reserves for its own cards.
pub(super) fn not_daemon_owned(kind: &String, (): &()) -> garde::Result {
    if DAEMON_OWNED_GATE_KINDS.contains(&kind.as_str()) {
        return Err(garde::Error::new(format!(
            "gate_kind {kind:?} is reserved for the daemon's own approval cards"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod reserved_kind_tests {
    use super::{DAEMON_OWNED_GATE_KINDS, not_daemon_owned};

    /// The escalation this validator exists to refuse.
    ///
    /// A fleet spelling the daemon's own kind raises a card that looks like a
    /// tool prompt and answers a credential question. Approving it would hand
    /// the fleet standing permission to mint a third party's credentials.
    #[test]
    fn a_fleet_may_not_author_a_daemon_owned_gate_kind() {
        for reserved in DAEMON_OWNED_GATE_KINDS {
            assert!(
                not_daemon_owned(&reserved.to_owned(), &()).is_err(),
                "{reserved} must be refused"
            );
        }
    }

    /// An ordinary kind still passes, so the guard refuses a set and not a shape.
    #[test]
    fn an_authored_kind_of_its_own_is_accepted() {
        for ordinary in [
            "deploy",
            "spend",
            "",
            "integration_grants",
            "repository_read",
        ] {
            assert!(
                not_daemon_owned(&ordinary.to_owned(), &()).is_ok(),
                "{ordinary} must be accepted"
            );
        }
    }

    /// Pins the spelling, because the owners cannot import it.
    ///
    /// `afd_gate::gate::detail::KIND_REPOSITORY_WRITE` and
    /// `afd_approval::KIND_INTEGRATION_GRANT` are the constants these strings
    /// stand in for, and both crates sit ABOVE this one so neither can be
    /// imported here. Each owner pins the same literal from its own side; a
    /// rename on either side fails a test rather than silently retiring the
    /// guard.
    #[test]
    fn the_reserved_set_is_exactly_the_two_kinds_the_daemon_raises() {
        assert_eq!(
            DAEMON_OWNED_GATE_KINDS,
            ["integration_grant", "repository_write"]
        );
    }
}
