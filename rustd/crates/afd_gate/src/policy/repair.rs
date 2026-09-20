//! The branch a write-bound run is allowed to author on.
//!
//! A repair branch names the EVENT the run is serving and nothing else: the
//! event's identifier, as its bytes, in unpadded URL-safe base64. It carries no
//! fleet and no workspace, so a name read off a public repository says when,
//! never whose.
//!
//! The identifier is TEXT, and that is not an encoding detail to route around.
//! `core.fleet_events` is keyed `(fleet_id, event_id)` with no surrogate — the
//! `id UUID` it once carried was deleted as a column "written by the insert and
//! read by nothing". Admission mints the key as `<millis>-<sequence>`, the shape
//! a Dragonfly stream entry id has, because every surface that renders, sorts or
//! pages on event ids was written against it. So there is no `Uuid7` in the
//! system that identifies a fleet event, and a version of this function that
//! asked for one could only ever be handed something else.
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

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The ref namespace every daemon-authored repair branch lives under.
pub const PREFIX: &str = "agentsfleet-repair/";

/// The shortest reference this encodes to, for a one-character identifier.
///
/// No longer a fixed width: event identifiers are variable-length text, so the
/// reference grows with the id it names rather than sitting at the twenty-two
/// characters sixteen raw bytes produced.
pub const MIN_REFERENCE_LEN: usize = 2;

/// The branch a lease authorises for `event_id`.
///
/// Infallible, and total over any identifier the events table can hold: the
/// encoding is applied to the bytes it is given rather than to a shape it
/// insists on. Two distinct identifiers cannot collide, because base64 is
/// injective — which is the whole property the egress lock rests on.
///
/// Absence is the CALLER's to express. A delivery with no usable identifier has
/// no branch, and [`Option`] says so at the call site (RULE FN-RS); returning a
/// guessed or empty name here would hand the egress rules a ref the run cannot
/// push to.
#[must_use]
pub fn branch_for(event_id: &str) -> String {
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(event_id.as_bytes()))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{MIN_REFERENCE_LEN, PREFIX, branch_for};

    /// An event identifier in the shape admission actually mints: the admission
    /// instant and the sequence, which is what `core.fleet_events` stores.
    const LEDGER_EVENT: &str = "1900000000000-1";

    #[test]
    fn a_branch_names_its_event_and_carries_nothing_else() {
        let branch = branch_for(LEDGER_EVENT);
        let reference = branch
            .strip_prefix(PREFIX)
            .expect("the namespace is present");

        assert!(reference.len() >= MIN_REFERENCE_LEN, "{branch}");
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
        assert_ne!(branch_for(LEDGER_EVENT), branch_for("1900000000000-2"));
    }

    #[test]
    fn one_event_always_names_the_same_branch() {
        // `policy::egress` locks this exact string as the only ref the run may
        // create, and the lease is assembled once per delivery. A name that
        // varied between two assemblies of one event would lock a branch the
        // run could not push to.
        assert_eq!(branch_for(LEDGER_EVENT), branch_for(LEDGER_EVENT));
    }

    #[test]
    fn test_m202_001_an_identifier_of_any_shape_is_encoded_whole() {
        // The regression. This took `&Uuid7`, so the call site parsed the event
        // id as one and dropped the branch when that failed — which it always
        // did, because the id is `<millis>-<sequence>` text and never a v7.
        // `core.fleet_events` holds other shapes too: an approval's
        // continuation, and every event predating the ledger. Each must get its
        // own branch rather than none.
        for id in [
            LEDGER_EVENT,
            "0197a4ba-8d3a-7f13-8abc-123456789abc",
            "evt-cred-mint-fixture",
        ] {
            let branch = branch_for(id);
            assert!(branch.starts_with(PREFIX), "{branch}");
            assert_ne!(branch, branch_for("1900000000000-999"), "{branch}");
        }
    }
}
