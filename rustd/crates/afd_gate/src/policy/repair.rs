//! The branch a write-bound run is allowed to author on.
//!
//! A repair branch names the EVENT the run is serving and nothing else: the
//! event's identifier, as its sixteen raw bytes, in unpadded URL-safe base64.
//! It carries no fleet and no workspace, so a name read off a public repository
//! says when, never whose.
//!
//! It named the approved repository-write gate until that gate was retired. The
//! standing integration grant could not take its place — one row per fleet and
//! service means one row for the fleet's whole life, so every event would
//! author on one branch and a second run would force-update the first run's
//! head. Authority is the grant, read at the mint; identity is the event, which
//! is unique per run. That is all the name has to carry.
//!
//! # Why the name has to be exact
//!
//! The branch is not a label. [`crate::policy::egress`] locks it into the
//! request rules as the only ref the run may create and the only head a Pull
//! Request may open from, so the approval a human gave — "one branch, one
//! draft Pull Request" — is enforceable precisely because no other branch name
//! is admitted. A run that could choose its own branch could write anywhere in
//! the repository the token reaches.
//!
//! # Nothing here is hand-decoded
//!
//! `repair_branch.zig` hand-writes the hex and base64 conversions because Zig
//! has neither in a form it can call. That is a constraint of the original,
//! not a property of the design: `uuid` and `base64` are already workspace
//! dependencies, they are tested far past what this module could justify
//! testing, and using them is what keeps the encoding the same on both sides.

use afd_core::id::Uuid7;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The ref namespace every daemon-authored repair branch lives under.
pub const PREFIX: &str = "agentsfleet-repair/";

/// The compact gate reference's width: sixteen bytes, unpadded base64.
pub const REFERENCE_LEN: usize = 22;

/// The branch a lease authorises for `event_id`.
///
/// Infallible, because the argument is already a validated identifier — the
/// Zig equivalent returns an error union only because it takes a string and
/// must re-check it. Taking the type instead moves that check to the one place
/// an identifier is made.
#[must_use]
pub fn branch_for(event_id: &Uuid7) -> String {
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(event_id.to_bytes()))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{PREFIX, REFERENCE_LEN, branch_for};
    use afd_core::id::Uuid7;

    /// An event identifier in the canonical spelling the ledger mints.
    fn event() -> Uuid7 {
        Uuid7::parse("0197a4ba-8d3a-7f13-8abc-123456789abc").expect("the fixture is a v7 spelling")
    }

    #[test]
    fn a_branch_names_its_event_and_carries_nothing_else() {
        let branch = branch_for(&event());
        let reference = branch
            .strip_prefix(PREFIX)
            .expect("the namespace is present");

        assert_eq!(reference.len(), REFERENCE_LEN);
        // Unpadded and URL-safe: a `=` or a `+` in a ref name is a branch git
        // will take and tooling will mangle.
        assert!(!branch.contains('='), "{branch}");
        assert!(!branch.contains('+'), "{branch}");
        assert_eq!(branch.matches('/').count(), 1, "{branch}");
    }

    #[test]
    fn test_m202_001_two_events_never_share_a_branch() {
        // The property that decided where this name comes from. The standing
        // grant is one row per fleet and service, so naming the branch after it
        // would put every event of a fleet's life on ONE branch — and the
        // second run would force-update the first run's head, on a repository
        // somebody else is reading. The event is unique per run, so the branch
        // is too.
        let first = Uuid7::parse("0197a4ba-8d3a-7f13-8abc-123456789abc").expect("a v7 spelling");
        let second = Uuid7::parse("0197a4ba-8d3a-7f13-8abc-123456789abd").expect("a v7 spelling");

        assert_ne!(branch_for(&first), branch_for(&second));
    }

    #[test]
    fn one_event_always_names_the_same_branch() {
        // `policy::egress` locks this exact string as the only ref the run may
        // create, and the lease is assembled once per delivery. A name that
        // varied between two assemblies of one event would lock a branch the
        // run could not push to.
        assert_eq!(branch_for(&event()), branch_for(&event()));
    }
}
