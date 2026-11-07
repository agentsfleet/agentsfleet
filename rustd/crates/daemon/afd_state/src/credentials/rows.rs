//! One Postgres row per credential class, and the record it becomes.
//!
//! # Why each class gets a derived `FromRow`
//!
//! Reading columns one at a time in the lookup means every read is its own
//! fallible step with its own error arm, and those arms are unreachable in a
//! correct build — the statement and the schema agree, or the migration that
//! broke them would have failed. A derived [`FromRow`] folds all of it into the
//! ONE failure `sqlx` already reports for a query, so the lookup has a single
//! error path and it is a path a test can reach by cutting the connection. Each
//! statement aliases its columns to these field names.
//!
//! # Why a dead row is returned
//!
//! A dead row is RETURNED, never filtered out. A revoked credential answers its
//! own code — `UZ-APIKEY-004`, `UZ-AUTH-023`, `UZ-RUN-009` — and a `WHERE
//! revoked_at IS NULL` would make it indistinguishable from a credential that never
//! existed, which is the difference between telling a terminal to stop and
//! letting it retry forever.

use crate::error::{Result, Unavailable};
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::principal::Subject;
use afd_core::error_code;
use afd_core::id::Uuid7;
use sqlx::FromRow;

/// The column a malformed tenant identifier is reported against.
const COLUMN_TENANT_ID: &str = "tenant_id";

/// The column a malformed runner identifier is reported against.
const COLUMN_ID: &str = "id";

/// `identity.api_keys`, joined to nothing.
///
/// `created_by_subject` holds the identity provider's subject claim, which is
/// why this class needs no join where [`CliCredentialRow`] does.
#[derive(Debug, FromRow)]
pub(super) struct TenantApiKeyRow {
    pub(super) tenant: String,
    pub(super) subject: String,
    /// `revoked_at IS NULL`.
    pub(super) usable: bool,
}

/// `identity.cli_credentials`, joined to the person who holds it.
///
/// Liveness is the NULLNESS of `revoked_at`, never its value: a row revoked
/// with a future instant is still revoked, and reading the instant would invite
/// a clock comparison the schema does not make either.
#[derive(Debug, FromRow)]
pub(super) struct CliCredentialRow {
    pub(super) tenant: String,
    pub(super) subject: String,
    /// `revoked_at IS NULL`.
    pub(super) usable: bool,
}

/// `runner.runners`, with the reconciliation verdict riding the same read.
#[derive(Debug, FromRow)]
pub(super) struct RunnerTokenRow {
    pub(super) runner: String,
    pub(super) status: String,
    /// `degraded_reason IS NOT NULL`.
    pub(super) degraded: bool,
}

impl RunnerTokenRow {
    /// Equal to active, rather than "not one of these five": a status added to
    /// the schema is then refused until somebody decides otherwise, which is
    /// the safe direction for an authority check.
    pub(super) fn live(&self) -> Liveness {
        liveness(self.status == crate::sql::RUNNER_STATUS_ACTIVE)
    }
}

/// `Live` when the row is usable, `Revoked` when it exists and is not.
pub(super) const fn liveness(usable: bool) -> Liveness {
    if usable {
        Liveness::Live
    } else {
        Liveness::Revoked
    }
}

/// Reads a canonical identifier out of a column, or refuses to answer.
///
/// A malformed identifier is a third thing the trait has no answer for, and
/// [`Unavailable`] is the least wrong of the two available. `Ok(None)` would
/// say the credential does not exist, sending a terminal into a retry loop
/// against a row that IS there — and for a runner it counts toward the
/// self-termination ceiling, so one corrupt row could walk a fleet to
/// shutdown. `Unavailable` is also simply true: the directory could not answer.
fn identifier(column: &'static str, value: &str) -> Result<Uuid7> {
    Uuid7::parse(value).map_err(|_malformed| {
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::error!(
            error_code = code,
            column,
            event = "credential_row_identity_invalid",
            "a credential row holds an identifier that is not a canonical \
             UUIDv7 — answering unavailable rather than reporting a credential \
             that exists as unknown"
        );
        Unavailable
    })
}

/// Reads a provider subject out of a row, or refuses to answer.
fn subject(value: &str) -> Result<Subject> {
    Subject::new(value).map_err(|_blank| {
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::error!(
            error_code = code,
            event = "credential_row_identity_blank",
            "a credential row holds a blank identity-provider subject — every \
             capability gate would refuse it, so the lookup answers unavailable \
             rather than authenticating a principal with no identity"
        );
        Unavailable
    })
}

/// The record for a person's credential.
pub(super) fn person(
    tenant: &str,
    subject_claim: &str,
    live: Liveness,
) -> Result<CredentialRecord> {
    Ok(CredentialRecord::Person {
        tenant: identifier(COLUMN_TENANT_ID, tenant)?,
        subject: subject(subject_claim)?,
        live,
    })
}

/// The record for a runner's credential.
pub(super) fn machine(runner: &str, degraded: bool, live: Liveness) -> Result<CredentialRecord> {
    Ok(CredentialRecord::Machine {
        runner: identifier(COLUMN_ID, runner)?,
        degraded,
        live,
    })
}
