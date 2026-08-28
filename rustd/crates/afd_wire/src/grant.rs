//! The integration-grant surface's payloads, and the column vocabulary two
//! planes read.
//!
//! A grant is a standing human decision about a fleet's relationship with a
//! third party: asked once, answered once, and consulted on every credential
//! mint after that. The operator lists them and revokes them; the runner plane
//! only ever asks whether one is approved.
//!
//! # Why the spellings live here rather than beside either reader
//!
//! Three crates touch `core.integration_grants.status` and none of them owns
//! it. `afd_gate` reads it on the mint hot path, `afd_approval` writes it —
//! both when an approval moves a grant and when an operator revokes one — and
//! the API renders it. This crate is the one they already share, so a spelling
//! declared here cannot drift into a row one plane writes and another cannot
//! read. It is the same arrangement, for the same reason, as
//! [`crate::approval::status`].
//!
//! # Field order is the contract
//!
//! `res.json` in `integration_grants/workspace.zig` emits `GrantRow`'s fields
//! in DECLARATION order, and a client reading the response as a positional
//! document would see a reorder. The order below is that one, and the absent
//! instants serialize as explicit `null` like everything else in this crate —
//! never `skip_serializing_if`.

use std::borrow::Cow;

use serde::Serialize;

/// The spellings `core.integration_grants.status` stores.
///
/// Three, and only one of them admits anything. `pending` is a grant the
/// install raised and nobody has answered; `approved` is the standing yes a
/// mint checks for; `revoked` is a yes a person took back. Absent, pending and
/// revoked are one answer to the runner plane and three different rows to the
/// operator, which is why the vocabulary is shared and the QUESTIONS are not.
pub mod status {
    /// Raised by the install, waiting on a human.
    pub const PENDING: &str = "pending";
    /// The standing yes a credential mint is gated on.
    pub const APPROVED: &str = "approved";
    /// A yes a person took back; nothing mints against it again.
    pub const REVOKED: &str = "revoked";
}

/// One grant as the fleet's list shows it.
///
/// `approved_at` and `revoked_at` are the two instants a grant's own
/// transitions stamp — the table carries no `updated_at`, because a row-change
/// time would say nothing those two do not already say. Both are `null` on a
/// pending row and exactly one is set afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrantSummary<'a> {
    /// The grant's own row id — what a revoke addresses it by.
    pub id: Cow<'a, str>,
    /// The third party the grant is about, as the credential classifier names it.
    pub service: Cow<'a, str>,
    /// Where the decision stands, from [`status`].
    pub status: Cow<'a, str>,
    /// When the grant was raised.
    pub created_at: i64,
    /// When a person approved it, or `null`.
    pub approved_at: Option<i64>,
    /// When a person took that back, or `null`.
    pub revoked_at: Option<i64>,
    /// Why the row exists at all — the install's own words, stored once.
    pub reason: Cow<'a, str>,
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants`.
///
/// `total` is the length of `items` and not a count of everything stored: the
/// list is unpaged, because a fleet holds at most one grant per service and the
/// supported-service count is what bounds it. Emitted anyway, because the Zig
/// handler emits it and a dashboard reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrantsResponse<'a> {
    /// The fleet's grants, newest first.
    pub items: Vec<GrantSummary<'a>>,
    /// How many rows `items` carries.
    pub total: usize,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{GrantSummary, GrantsResponse, status};
    use std::borrow::Cow;

    #[test]
    fn a_fleet_holding_no_grants_answers_an_empty_array_and_a_zero() {
        // Never `null`, and never an omitted `total`. A dashboard iterating the
        // list should not have to branch on its absence first, and
        // `.{ .items = grants.items, .total = grants.items.len }` emits `[]`
        // and `0` for an empty slice too.
        let body = serde_json::to_string(&GrantsResponse {
            items: vec![],
            total: 0,
        })
        .expect("the response serializes");

        assert_eq!(body, r#"{"items":[],"total":0}"#);
    }

    #[test]
    fn a_pending_grant_emits_both_instants_as_explicit_nulls() {
        // The whole reason this crate forbids `skip_serializing_if`: the Zig
        // emitter writes `null` for an absent optional, and a dropped key would
        // change the document a client parses without changing any value in it.
        let body = serde_json::to_string(&GrantSummary {
            id: Cow::Borrowed("01924f4e-0000-7000-8000-0000000000a1"),
            service: Cow::Borrowed("slack"),
            status: Cow::Borrowed(status::PENDING),
            created_at: 1_777_507_200_000,
            approved_at: None,
            revoked_at: None,
            reason: Cow::Borrowed("Declared by the fleet bundle at install"),
        })
        .expect("the row serializes");

        assert_eq!(
            body,
            r#"{"id":"01924f4e-0000-7000-8000-0000000000a1","service":"slack","status":"pending","created_at":1777507200000,"approved_at":null,"revoked_at":null,"reason":"Declared by the fleet bundle at install"}"#
        );
    }

    #[test]
    fn a_revoked_grant_keeps_the_approval_it_once_had() {
        // Revoking stamps `revoked_at` and leaves `approved_at` alone, so the
        // row still says a person once said yes. A revoke that cleared it would
        // make a taken-back approval indistinguishable from one never given.
        let body = serde_json::to_string(&GrantSummary {
            id: Cow::Borrowed("01924f4e-0000-7000-8000-0000000000a2"),
            service: Cow::Borrowed("github"),
            status: Cow::Borrowed(status::REVOKED),
            created_at: 1,
            approved_at: Some(2),
            revoked_at: Some(3),
            reason: Cow::Borrowed("asked at install"),
        })
        .expect("the row serializes");

        assert_eq!(
            body,
            r#"{"id":"01924f4e-0000-7000-8000-0000000000a2","service":"github","status":"revoked","created_at":1,"approved_at":2,"revoked_at":3,"reason":"asked at install"}"#
        );
    }

    #[test]
    fn the_status_vocabulary_is_the_columns_exact_spellings() {
        // Pinned against the strings, because these are what the schema holds
        // and what `integration_grant_lookup.zig` writes. A rename here would
        // silently stop matching every stored row.
        assert_eq!(status::PENDING, "pending");
        assert_eq!(status::APPROVED, "approved");
        assert_eq!(status::REVOKED, "revoked");
    }
}
