//! The integration-grant surface's payloads, and the column vocabulary three
//! crates read.
//!
//! A grant gives one fleet one credential: a workspace secret or a connected
//! integration such as GitHub. The fleet asks for it by name, a person
//! approves it once on a card, and every run and credential mint after that
//! reads it. A person lists a fleet's grants and revokes one.
//!
//! # Why the spellings live here rather than beside either reader
//!
//! `afd_gate` reads `connector.integration_grants.status` and `grant_type` on the
//! policy and mint paths, `afd_approval` writes them, and the API renders
//! them. This crate is the one they already share, so a spelling declared
//! here cannot drift into a row one plane writes and another cannot read. It
//! is the same arrangement, for the same reason, as
//! [`crate::approval::status`].
//!
//! # Field order is the contract
//!
//! `Grant` serializes its fields in DECLARATION order. Absent values serialize
//! as explicit `null` like everything else in this crate — never
//! `skip_serializing_if`.

use std::borrow::Cow;

use serde::Serialize;

/// The spellings `connector.integration_grants.status` stores.
///
/// Three, and only one of them admits anything. `pending` is a grant a fleet
/// asked for and nobody has answered; `approved` is the standing yes a run and
/// a mint read; `revoked` is a no, or a yes a person took back. A fleet that
/// asks again moves a revoked grant back to `pending`.
pub mod status {
    /// Asked for by the fleet, waiting on a person.
    pub const PENDING: &str = "pending";
    /// The standing yes a run's policy and a credential mint read.
    pub const APPROVED: &str = "approved";
    /// Denied, or a yes a person took back; nothing reads it as access.
    pub const REVOKED: &str = "revoked";
}

/// The spellings `connector.integration_grants.grant_type` stores: which channel the
/// approved credential reaches a run through.
pub mod grant_type {
    /// A DNS host the fleet can access through its web tools.
    pub const HOST: &str = "host";
    /// A workspace vault entry the run's tools address by field.
    pub const SECRET: &str = "secret";
    /// A connected integration whose token is minted per run, such as GitHub.
    pub const INTEGRATION: &str = "integration";
}

/// One grant as the fleet's list shows it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Grant<'a> {
    /// The grant's own row id — what a revoke addresses it by.
    pub id: Cow<'a, str>,
    /// `secret`, `integration` or `host`, from [`grant_type`].
    pub kind: Cow<'a, str>,
    /// The vault key or integration the fleet asked for.
    pub name: Cow<'a, str>,
    /// Where the decision stands, from [`status`].
    pub status: Cow<'a, str>,
    /// The GitHub scope in force, `{repositories, access, base?}`, or `null`.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub scope: Option<serde_json::Value>,
    /// A wider GitHub scope waiting on a person, or `null`.
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub requested_scope: Option<serde_json::Value>,
    /// When a person approved it, or `null`.
    #[serde(with = "af_wire::instant::option", default)]
    pub approved_at: Option<jiff::Timestamp>,
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/integration-grants`.
//
// Each entry is identified by its grant ID. A host and a credential
// can share a name while having independent approvals.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrantsResponse<'a> {
    /// The fleet's grants, newest first.
    pub items: Vec<Grant<'a>>,
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{Grant, GrantsResponse, grant_type, status};
    use std::borrow::Cow;

    #[test]
    fn a_fleet_holding_no_grants_answers_an_empty_array() {
        let body = serde_json::to_string(&GrantsResponse { items: vec![] })
            .expect("the response serializes");

        assert_eq!(body, r#"{"items":[]}"#);
    }

    #[test]
    fn a_pending_secret_emits_its_absent_values_as_explicit_nulls() {
        // The whole reason this crate forbids `skip_serializing_if`: a dropped
        // key would change the document a client parses without changing any
        // value in it.
        let body = serde_json::to_string(&Grant {
            id: Cow::Borrowed("01924f4e-0000-7000-8000-0000000000a1"),
            kind: Cow::Borrowed(grant_type::SECRET),
            name: Cow::Borrowed("sentry"),
            status: Cow::Borrowed(status::PENDING),
            scope: None,
            requested_scope: None,
            approved_at: None,
        })
        .expect("the row serializes");

        assert_eq!(
            body,
            r#"{"id":"01924f4e-0000-7000-8000-0000000000a1","kind":"secret","name":"sentry","status":"pending","scope":null,"requested_scope":null,"approved_at":null}"#
        );
    }

    #[test]
    fn a_wider_github_request_shows_beside_the_scope_in_force() {
        let body = serde_json::to_value(Grant {
            id: Cow::Borrowed("01924f4e-0000-7000-8000-0000000000a2"),
            kind: Cow::Borrowed(grant_type::INTEGRATION),
            name: Cow::Borrowed("github"),
            status: Cow::Borrowed(status::APPROVED),
            scope: Some(serde_json::json!({"repositories": ["org/app"], "access": "read"})),
            requested_scope: Some(
                serde_json::json!({"repositories": ["org/app"], "access": "write", "base": "main"}),
            ),
            approved_at: Some(jiff::Timestamp::constant(2, 0)),
        })
        .expect("the row serializes");

        let read = |pointer: &str| body.pointer(pointer).and_then(serde_json::Value::as_str);
        assert_eq!(read("/scope/access"), Some("read"));
        assert_eq!(read("/requested_scope/access"), Some("write"));
        assert_eq!(read("/approved_at"), Some("1970-01-01T00:00:02Z"));
    }

    #[test]
    fn the_vocabularies_are_the_columns_exact_spellings() {
        // Pinned against the strings, because these are what the schema holds
        // and what `afd_approval` writes. A rename here would silently stop
        // matching every stored row.
        assert_eq!(status::PENDING, "pending");
        assert_eq!(status::APPROVED, "approved");
        assert_eq!(status::REVOKED, "revoked");
        assert_eq!(grant_type::SECRET, "secret");
        assert_eq!(grant_type::INTEGRATION, "integration");
    }
}
