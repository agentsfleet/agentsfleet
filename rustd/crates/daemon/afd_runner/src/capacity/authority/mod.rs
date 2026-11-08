//! A connection proves its credential and ownership again at every grant boundary.

mod sql;

use std::fmt;

use af_core::{
    error_code::{self, ErrorCode},
    id::Uuid7,
};
use afd_auth::directory::Digest;
use jiff::Timestamp;
use sqlx::PgConnection;

use crate::{Result, Runners, error::query};

/// A durable refusal is distinct from an unavailable database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The credential no longer identifies an enrolled host.
    Credential,
    /// The operator stopped this host.
    Revoked,
    /// Another connection acquired ownership after this one.
    Superseded,
}

impl Refusal {
    /// Stable machine code with a corresponding human-readable problem entry.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::Credential => error_code::RUN_INVALID_RUNNER_TOKEN,
            Self::Revoked => error_code::RUN_ADMIN_STATE_BLOCKED,
            Self::Superseded => error_code::RUN_STREAM_SUPERSEDED,
        }
    }
}

/// An authority check either proves its value or gives the exact terminal refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<T> {
    /// The guarded operation may proceed with this value.
    Allowed(T),
    /// The operation must stop; retrying the same connection cannot help.
    Refused(Refusal),
}

/// The database-issued owner; credential digests never appear in its debug output.
#[derive(Clone, PartialEq, Eq)]
pub struct Authority {
    runner: Uuid7,
    connection: Uuid7,
    digest: Digest,
}

impl fmt::Debug for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authority")
            .field("runner", &self.runner)
            .field("connection", &self.connection)
            .finish_non_exhaustive()
    }
}

impl Authority {
    /// Enrolled host whose transactions this connection may authorize.
    #[must_use]
    pub const fn runner(&self) -> &Uuid7 {
        &self.runner
    }

    /// Native `UUIDv7` used to bind the remaining lanes and fence old connections.
    #[must_use]
    pub const fn connection(&self) -> &Uuid7 {
        &self.connection
    }

    pub(super) fn guard_for<'a>(&'a self, runner: &'a Uuid7) -> super::Guard<'a> {
        super::Guard::Connected {
            runner,
            connection: &self.connection,
            digest: &self.digest,
        }
    }
}

#[derive(sqlx::FromRow)]
struct Checked {
    credential_matches: bool,
    active: bool,
    owned: bool,
}

impl Checked {
    fn decision(self) -> Decision<()> {
        match (self.credential_matches, self.active, self.owned) {
            (false, _, _) => Decision::Refused(Refusal::Credential),
            (true, false, _) => Decision::Refused(Refusal::Revoked),
            (true, true, false) => Decision::Refused(Refusal::Superseded),
            (true, true, true) => Decision::Allowed(()),
        }
    }
}

impl Runners {
    /// Acquire ownership only while the opening credential still proves an active row.
    /// Automatic reconnect supplies its previous owner; a fresh process omits it.
    /// The marker outlives disconnect so a replaced process cannot resume over its replacement.
    ///
    /// # Errors
    /// Preserves database and entropy failures; durable refusals are returned as decisions.
    pub async fn acquire_connection(
        &self,
        runner: &Uuid7,
        digest: Digest,
        previous: Option<&Uuid7>,
        now: Timestamp,
    ) -> Result<Decision<Authority>> {
        let connection = self.entropy().uuid7(now)?;
        let row = sqlx::query_as::<_, Checked>(sql::ACQUIRE)
            .bind(runner)
            .bind(digest.as_str())
            .bind(&connection)
            .bind(crate::sql::RUNNER_STATUS_ACTIVE)
            .bind(previous)
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(match row.map(Checked::decision) {
            Some(Decision::Allowed(())) => Decision::Allowed(Authority {
                runner: runner.clone(),
                connection,
                digest,
            }),
            Some(Decision::Refused(refusal)) => Decision::Refused(refusal),
            None => Decision::Refused(Refusal::Credential),
        })
    }

    /// Compare the credential and owner without deriving authority from broker delivery.
    ///
    /// # Errors
    /// Preserves database failures, so an unavailable store cannot masquerade as revocation.
    pub async fn check_connection(&self, authority: &Authority) -> Result<Decision<()>> {
        check_on(&mut *self.pool().acquire().await?, authority).await
    }
}

async fn check_on(connection: &mut PgConnection, authority: &Authority) -> Result<Decision<()>> {
    let row = sqlx::query_as::<_, Checked>(sql::CHECK)
        .bind(authority.runner())
        .bind(authority.digest.as_str())
        .bind(authority.connection())
        .bind(crate::sql::RUNNER_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(row.map_or(Decision::Refused(Refusal::Credential), Checked::decision))
}
