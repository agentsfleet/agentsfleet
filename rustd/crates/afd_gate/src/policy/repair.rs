//! The branch a write-bound run is allowed to author on.
//!
//! A repair branch names the approved repository-write gate and nothing else:
//! the gate's identifier, as its sixteen raw bytes, in unpadded URL-safe
//! base64. It carries no fleet and no event — the daemon resolves those from
//! the gate row — so the branch cannot be read for tenant identity by anyone
//! who sees it on a repository.
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

use afd_core::id::{BYTE_LEN, Uuid7};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The ref namespace every daemon-authored repair branch lives under.
pub const PREFIX: &str = "agentsfleet-repair/";

/// The compact gate reference's width: sixteen bytes, unpadded base64.
pub const REFERENCE_LEN: usize = 22;

/// The branch a lease authorises for `gate_id`.
///
/// Infallible, because the argument is already a validated identifier — the
/// Zig equivalent returns an error union only because it takes a string and
/// must re-check it. Taking the type instead moves that check to the one place
/// an identifier is made.
#[must_use]
pub fn branch_for(gate_id: &Uuid7) -> String {
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(gate_id.to_bytes()))
}

/// The gate `branch` names, if it names one exactly.
///
/// `None` for every alias, and that strictness is the security property rather
/// than tidiness: padding, a differing length, or any other encoding of the
/// same bytes would let two distinct branch names resolve to one approved
/// gate, and only one of them is the name the egress rules admit. So the
/// reference must be the CANONICAL encoding of what it decodes to — which is
/// checked by re-encoding, not by trusting the decoder to be strict.
#[must_use]
pub fn gate_of(branch: &str) -> Option<Uuid7> {
    let reference = branch.strip_prefix(PREFIX)?;
    if reference.len() != REFERENCE_LEN {
        return None;
    }
    let raw: [u8; BYTE_LEN] = URL_SAFE_NO_PAD.decode(reference).ok()?.try_into().ok()?;
    if URL_SAFE_NO_PAD.encode(raw) != reference {
        return None;
    }
    // Rejects any identifier that is not a version-7 UUID, which is what the
    // gate table stores. A decoded value that is not one was never a gate.
    Uuid7::parse(&uuid::Uuid::from_bytes(raw).to_string()).ok()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{PREFIX, REFERENCE_LEN, branch_for, gate_of};
    use afd_core::id::Uuid7;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    /// A gate identifier in the canonical spelling the table stores.
    fn gate() -> Uuid7 {
        Uuid7::parse("0197a4ba-8d3a-7f13-8abc-123456789abc").expect("the fixture is a v7 spelling")
    }

    #[test]
    fn a_branch_names_its_gate_and_carries_nothing_else() {
        let branch = branch_for(&gate());
        let reference = branch
            .strip_prefix(PREFIX)
            .expect("the namespace is present");

        assert_eq!(reference.len(), REFERENCE_LEN);
        // Unpadded and URL-safe: a `=` or a `+` in a ref name is a branch git
        // will take and tooling will mangle.
        assert!(!branch.contains('='), "{branch}");
        assert!(!branch.contains('+'), "{branch}");
        assert!(
            !branch.contains('/') || branch.matches('/').count() == 1,
            "{branch}"
        );
    }

    #[test]
    fn a_branch_round_trips_back_to_the_gate_it_names() {
        let gate = gate();

        assert_eq!(gate_of(&branch_for(&gate)).as_ref(), Some(&gate));
    }

    #[test]
    fn an_alias_of_the_same_bytes_is_refused() {
        // The property that matters. Two spellings resolving to one approved
        // gate would mean a branch the egress rules do NOT admit could still
        // be presented as authorised.
        let branch = branch_for(&gate());

        assert!(gate_of(&format!("{branch}=")).is_none(), "padded");
        assert!(gate_of(&branch[..branch.len() - 1]).is_none(), "truncated");
        assert!(gate_of(&format!("{branch}a")).is_none(), "extended");
    }

    #[test]
    fn a_branch_outside_the_namespace_names_no_gate() {
        let reference = branch_for(&gate())
            .strip_prefix(PREFIX)
            .expect("the namespace is present")
            .to_owned();

        assert!(gate_of(&reference).is_none(), "no namespace");
        assert!(
            gate_of(&format!("feature/{reference}")).is_none(),
            "another namespace"
        );
        assert!(gate_of("").is_none());
        assert!(gate_of(PREFIX).is_none());
    }

    #[test]
    fn a_reference_that_is_not_a_version_seven_identifier_names_no_gate() {
        // The gate table stores v7 identifiers. Sixteen bytes that decode to
        // anything else were never a gate, and admitting them would hand the
        // lookup a value it cannot find and cannot explain.
        let not_v7 = URL_SAFE_NO_PAD.encode([0u8; 16]);

        assert_eq!(not_v7.len(), REFERENCE_LEN);
        assert!(gate_of(&format!("{PREFIX}{not_v7}")).is_none());
    }
}
