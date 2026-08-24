//! One Postgres row per credential class, and the record it becomes.
//!
//! # Why each class gets a `FromRow` rather than a bag of `try_get` calls
//!
//! Reading columns one at a time in the lookup means every read is its own
//! fallible step with its own error arm, and those arms are unreachable in a
//! correct build — the statement and the schema agree, or the migration that
//! broke them would have failed. Unreachable error handling is still dead code,
//! and this workspace measures that.
//!
//! A hand-written [`FromRow`] folds all of it into the ONE failure `sqlx`
//! already reports for a query, so the lookup has a single error path and it is
//! a path a test can reach by cutting the connection. It also puts each class's
//! column names and its liveness rule in one place, which is where a reader
//! looking for "what does revoked mean for a command-line credential" would go.
//!
//! Written out rather than derived: the derive needs `sqlx`'s `macros` feature
//! and another proc-macro chain, which this workspace declines for the same
//! reason the problem+json envelope builds a `Map` instead of deriving
//! `Serialize`.
//!
//! # Why liveness is decided here and not in SQL
//!
//! A dead row is RETURNED, never filtered out. A revoked credential answers its
//! own code — `UZ-APIKEY-004`, `UZ-AUTH-023`, `UZ-RUN-009` — and a `WHERE
//! active = TRUE` would make it indistinguishable from a credential that never
//! existed, which is the difference between telling a terminal to stop and
//! letting it retry forever.

use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::error::Unavailable;
use afd_auth::principal::Subject;
use afd_core::error_code;
use afd_core::id::Uuid7;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Row as _};

/// `core.api_keys`, joined to nothing.
///
/// `created_by` is `TEXT NOT NULL` holding the identity provider's subject
/// claim — `schema/240_api_keys.sql` says so above the column — which is why
/// this class needs no join where [`CliCredentialRow`] does.
#[derive(Debug)]
pub(super) struct TenantApiKeyRow {
    pub(super) tenant: String,
    pub(super) subject: String,
    pub(super) live: Liveness,
}

impl FromRow<'_, PgRow> for TenantApiKeyRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            tenant: row.try_get("tenant_id")?,
            subject: row.try_get("created_by")?,
            live: liveness(row.try_get::<bool, _>("active")?),
        })
    }
}

/// `core.cli_credentials`, joined to the person who holds it.
#[derive(Debug)]
pub(super) struct CliCredentialRow {
    pub(super) tenant: String,
    pub(super) subject: String,
    pub(super) live: Liveness,
}

impl FromRow<'_, PgRow> for CliCredentialRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        // Liveness is the NULLNESS of the timestamp, never its value. A row
        // revoked with a future instant is still revoked here, and reading the
        // instant would invite a clock comparison the Zig lookup does not make
        // either — `revoked_at IS NULL` is what its own partial unique index
        // is built on.
        let revoked_at: Option<i64> = row.try_get("revoked_at")?;
        Ok(Self {
            tenant: row.try_get("tenant_id")?,
            subject: row.try_get("oidc_subject")?,
            live: liveness(revoked_at.is_none()),
        })
    }
}

/// `fleet.runners`, with the reconciliation verdict riding the same read.
#[derive(Debug)]
pub(super) struct RunnerTokenRow {
    pub(super) runner: String,
    pub(super) degraded: bool,
    pub(super) live: Liveness,
}

impl FromRow<'_, PgRow> for RunnerTokenRow {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let admin_state: String = row.try_get("admin_state")?;
        Ok(Self {
            runner: row.try_get("id")?,
            degraded: row.try_get("degraded")?,
            // Equal to active, rather than "not one of these five": a state
            // added to the schema is then refused until somebody decides
            // otherwise, which is the safe direction for an authority check.
            live: liveness(admin_state == crate::sql::ADMIN_STATE_ACTIVE),
        })
    }
}

/// `Live` when the row is usable, `Revoked` when it exists and is not.
const fn liveness(usable: bool) -> Liveness {
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
fn identifier(column: &'static str, value: &str) -> Result<Uuid7, Unavailable> {
    Uuid7::parse(value).map_err(|_malformed| {
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::error!(
            error_code = code,
            column,
            "a credential row holds an identifier that is not a canonical \
             UUIDv7 — answering unavailable rather than reporting a credential \
             that exists as unknown"
        );
        Unavailable
    })
}

/// Reads a provider subject out of a row, or refuses to answer.
fn subject(value: &str) -> Result<Subject, Unavailable> {
    Subject::new(value).map_err(|_blank| {
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        tracing::error!(
            error_code = code,
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
) -> Result<CredentialRecord, Unavailable> {
    Ok(CredentialRecord::Person {
        tenant: identifier("tenant_id", tenant)?,
        subject: subject(subject_claim)?,
        live,
    })
}

/// The record for a runner's credential.
pub(super) fn machine(
    runner: &str,
    degraded: bool,
    live: Liveness,
) -> Result<CredentialRecord, Unavailable> {
    Ok(CredentialRecord::Machine {
        runner: identifier("id", runner)?,
        degraded,
        live,
    })
}
