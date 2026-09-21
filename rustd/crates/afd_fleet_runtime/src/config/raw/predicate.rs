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

/// The one gate kind only the daemon may raise.
///
/// Spelled here rather than imported because this crate and `afd_approval` are
/// SIBLINGS — neither names the other in its manifest, so the constant
/// `afd_approval` owns (`KIND_INTEGRATION_GRANT`) cannot be reached from this
/// side. Each crate holds the literal and pins it in its own test, so renaming
/// one alone fails a build rather than silently retiring the guard.
///
/// It was a two-element list until the second kind was retired: the standing
/// integration grant now authorises a repository write, no daemon path raises
/// that card, and the spelling had no owning constant left to reserve.
pub(super) const DAEMON_OWNED_GATE_KIND: &str = "integration_grant";

/// Refuses the gate kind the daemon reserves for its own cards.
pub(super) fn not_daemon_owned(kind: &String, (): &()) -> garde::Result {
    if kind == DAEMON_OWNED_GATE_KIND {
        return Err(garde::Error::new(format!(
            "gate_kind {kind:?} is reserved for the daemon's own approval cards"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod reserved_kind_tests {
    use super::{DAEMON_OWNED_GATE_KIND, not_daemon_owned};

    /// The escalation this validator exists to refuse.
    ///
    /// A fleet spelling the daemon's own kind raises a card that looks like a
    /// tool prompt and answers a credential question. Approving it would hand
    /// the fleet standing permission to mint a third party's credentials.
    #[test]
    fn a_fleet_may_not_author_a_daemon_owned_gate_kind() {
        assert!(
            not_daemon_owned(&DAEMON_OWNED_GATE_KIND.to_owned(), &()).is_err(),
            "{DAEMON_OWNED_GATE_KIND} must be refused"
        );
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

    /// Pins the spelling, because the owner cannot import it.
    ///
    /// `afd_approval::KIND_INTEGRATION_GRANT` is the constant this string
    /// stands in for, and that crate sits ABOVE this one so it cannot be
    /// imported here. It pins the same literal from its own side; a rename
    /// there fails a test rather than silently retiring the guard.
    ///
    /// A near-miss spelling is accepted, and that is deliberate.
    ///
    /// The guard matches the reserved kind EXACTLY, so a case variant or a
    /// padded spelling passes it. That is safe rather than a hole, and the
    /// reason is the resolve on the other side: `afd_approval` binds its own
    /// `KIND_INTEGRATION_GRANT` constant into the statement that moves a
    /// credential grant, so a card carrying a near-miss kind matches nothing
    /// there and moves no grant. Pinned here because the guard changed from a
    /// list membership test to an equality test — if a future reader "fixes"
    /// this by lowercasing or trimming, they widen a refusal past the one
    /// spelling that can actually reach the grant path, and start refusing
    /// fleets names nothing claims.
    #[test]
    fn a_near_miss_spelling_is_accepted_because_it_reaches_no_grant() {
        for near_miss in [
            "Integration_Grant",
            "INTEGRATION_GRANT",
            " integration_grant",
            "integration_grant ",
            "integration-grant",
        ] {
            assert!(
                not_daemon_owned(&near_miss.to_owned(), &()).is_ok(),
                "{near_miss} is not the reserved spelling and must be accepted"
            );
        }
    }

    /// A kind the daemon no longer raises must not stay reserved: that would
    /// refuse a fleet a name nothing else claims. One was retired, and the
    /// scalar is what keeps a second from creeping back without a daemon path
    /// behind it — there is no list to append to.
    #[test]
    fn the_reserved_kind_is_the_one_the_daemon_raises() {
        assert_eq!(DAEMON_OWNED_GATE_KIND, "integration_grant");
    }
}
