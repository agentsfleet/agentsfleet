//! The two claims that bound a token's authority: the workspace ceiling, and
//! the instants the token is valid between.
//!
//! Separate from `claim_shapes` because the question is different. That file
//! asks what a claim SAYS; this asks what the daemon does when a claim tries to
//! LIMIT what the holder can reach and the daemon cannot act on it. The
//! distinction has a failure mode attached: for a claim that grants, being
//! unreadable and being absent both end in a refusal, so conflating them costs
//! nothing — for a claim that restricts, absence means unrestricted, and
//! conflating them hands out exactly the access the claim existed to withhold.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::verifier::VerifyError;

mod support;
use support::signing::{
    AUDIENCE, ISSUER, NOT_EXPIRED, NOW_MS, NOW_S, TENANT, WORKSPACE, verify, verify_at,
};

/// A well-formed payload carrying `extra` verbatim before the closing brace.
///
/// One spelling of the envelope, so a test that fails is failing about the
/// claim it names and not about a comma (RULE UFS).
fn payload_with(extra: &str) -> String {
    format!(
        "{{\"sub\":\"user_bounded\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
         \"exp\":{NOT_EXPIRED},\"metadata\":{{\"tenant_id\":\"{TENANT}\"}}{extra}}}"
    )
}

// ── The workspace ceiling ────────────────────────────────────────────────

/// A ceiling the daemon cannot read REFUSES the token.
///
/// The regression this file exists for. Before the fix `Uuid7::parse(..).ok()`
/// turned an unreadable ceiling into `None`, and `None` is what a token with no
/// ceiling at all carries — so `WorkspaceDirectory::authorize` skipped the
/// confinement check entirely and the holder reached every workspace their
/// other claims allowed. Nothing logged, because from the authoriser's side a
/// dropped ceiling and an absent one are the same value.
///
/// `ws_a` is not an arbitrary bad string: it is how every workspace identifier
/// in the Zig tree's own claim fixtures is spelled, which makes it the spelling
/// an implementer wiring this claim up is most likely to reach for first.
#[test]
fn test_an_unreadable_ceiling_refuses_the_token() {
    let refused = verify(&payload_with(",\"workspace_id\":\"ws_a\""))
        .expect_err("a restriction this daemon cannot apply must not be ignored");

    assert_eq!(refused, VerifyError::UnreadableCeiling);
}

/// Every near-miss spelling refuses, not only an obviously wrong one.
///
/// The identifier is canonical-only — lowercase, version nibble 7, RFC 4122
/// variant — so the failures worth pinning are the ones that LOOK right. A
/// version-4 identifier is the case with teeth: it is a real identifier from a
/// real generator, and it is refused here for the version nibble alone.
#[test]
fn test_a_ceiling_in_any_near_miss_spelling_is_refused() {
    for spelling in [
        // A genuine UUID, wrong version — what a v4 generator hands you.
        "0199a1b2-c3d4-4e5f-8a9b-0c1d2e3f4a7d",
        // Canonical but uppercase.
        "0199A1B2-C3D4-7E5F-8A9B-0C1D2E3F4A7D",
        // The right identifier wearing a prefix.
        "ws_0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d",
        // Truncated in transit.
        "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a",
        // Present but empty, which is not "unset".
        "",
    ] {
        let refused = verify(&payload_with(&format!(",\"workspace_id\":\"{spelling}\"")))
            .expect_err("an unreadable ceiling is refused");

        assert_eq!(refused, VerifyError::UnreadableCeiling, "{spelling:?}");
    }
}

/// An ABSENT ceiling still means no ceiling.
///
/// The other half of the fix, and the half that would break every token in
/// service if it were got wrong: nothing writes this claim today, so a daemon
/// that refused on absence would refuse every session token there is.
#[test]
fn test_an_absent_ceiling_leaves_the_principal_unconfined() {
    let claims = verify(&payload_with("")).expect("a token with no ceiling is a normal token");

    assert!(claims.workspace_scope.is_none());
}

/// A readable ceiling arrives intact, so the refusal above is about shape.
#[test]
fn test_a_readable_ceiling_is_carried_through() {
    let claims = verify(&payload_with(&format!(",\"workspace_id\":\"{WORKSPACE}\"")))
        .expect("a canonical ceiling verifies");

    assert_eq!(
        claims
            .workspace_scope
            .as_ref()
            .map(afd_core::id::Uuid7::as_str),
        Some(WORKSPACE)
    );
}

/// The ceiling is read through the same nested ladder the tenant is.
///
/// A template that projects the ceiling under `metadata` rather than at the top
/// level must get the same enforcement, or the fix would hold for one
/// projection shape and silently not for the other.
#[test]
fn test_a_ceiling_nested_under_metadata_is_read_and_enforced() {
    let nested = format!(
        "{{\"sub\":\"user_bounded\",\"iss\":\"{ISSUER}\",\"aud\":\"{AUDIENCE}\",\
         \"exp\":{NOT_EXPIRED},\"metadata\":{{\"tenant_id\":\"{TENANT}\",\
         \"workspace_id\":\"ws_nested\"}}}}"
    );

    let refused = verify(&nested).expect_err("a nested ceiling is a ceiling");
    assert_eq!(refused, VerifyError::UnreadableCeiling);
}

// ── The instants a token is valid between ────────────────────────────────

/// A token whose `nbf` has not arrived is refused.
#[test]
fn test_a_token_that_is_not_valid_yet_is_refused() {
    let refused = verify(&payload_with(&format!(",\"nbf\":{}", NOW_S + 60)))
        .expect_err("a token that is not valid yet is not one to act on");

    assert_eq!(refused, VerifyError::NotYetValid);
}

/// `nbf` exactly now is valid — the boundary is `>`, not `>=`.
///
/// Opposite to `exp`, deliberately: a token is valid FROM `nbf` and UNTIL
/// `exp`, so the instant itself belongs to the valid side here and to the
/// expired side there. Pinning both boundaries is what stops the two checks
/// being made consistent with each other and wrong.
#[test]
fn test_a_token_valid_from_exactly_now_is_accepted() {
    verify(&payload_with(&format!(",\"nbf\":{NOW_S}")))
        .expect("a token valid from this very instant is valid");
}

/// An `nbf` already in the past does not interfere.
///
/// The shape the configured provider actually sends — its session tokens carry
/// `nbf` ten seconds behind `iat` on every mint — so this is the case that
/// proves the new check cannot reject production traffic.
#[test]
fn test_a_token_whose_validity_already_began_verifies() {
    let claims = verify(&payload_with(&format!(",\"nbf\":{}", NOW_S - 10)))
        .expect("a token already in its validity window verifies");

    assert!(claims.tenant.is_some());
}

/// A token with no `nbf` at all verifies — the claim is optional.
#[test]
fn test_a_token_with_no_validity_start_verifies() {
    verify(&payload_with("")).expect("nbf is checked when present, never required");
}

/// The clock decides `nbf`, not the wall clock.
///
/// The same token refused at one instant and accepted at another, which is the
/// property that would be lost by delegating this check to a crate that reads
/// `SystemTime` with no seam.
#[test]
fn test_validity_start_is_decided_by_the_injected_clock() {
    let token = payload_with(&format!(",\"nbf\":{}", NOW_S + 60));

    let refused = verify_at(&token, NOW_MS).expect_err("before the window opens");
    assert_eq!(refused, VerifyError::NotYetValid);

    verify_at(&token, NOW_MS + 120_000).expect("after the window opens");
}

// ── The payload's shape at the trust boundary ────────────────────────────

/// A POPULATED positional array is refused as a payload.
///
/// `claim_shapes` pins `[]`, which cannot fill the claim struct whatever reader
/// is used — it fails on arity alone, so it would stay green through any change
/// here. An array with a value per field is the case with something to say.
///
/// **This passes with either reader today, and that is the point.** Two things
/// independently refuse it: the object-only reader, and — incidentally —
/// `#[serde(flatten)]` on `Claims::rest`, which suppresses serde's sequence
/// path as a side effect of existing for an unrelated reason. Removing the
/// flattened field is an ordinary refactor that would silently withdraw the
/// second guard, and this test is what makes that refactor fail loudly instead
/// of quietly re-opening a positional read of the authorisation claims.
#[test]
fn test_a_populated_positional_array_payload_is_refused() {
    let positional =
        format!("[\"user_x\",\"{ISSUER}\",\"{AUDIENCE}\",{NOT_EXPIRED},null,\"admin\"]");

    let refused = verify(&positional).expect_err("a claim set is an object, never a sequence");
    assert_eq!(refused, VerifyError::Malformed);
}
