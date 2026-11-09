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

/// A prospective owner that has not changed the database yet.
/// The runner records both identifiers before confirming over its native connection.
pub struct Prepared {
    runner: Uuid7,
    previous: Option<Uuid7>,
    connection: Uuid7,
    digest: Digest,
}

impl fmt::Debug for Prepared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prepared")
            .field("runner", &self.runner)
            .field("previous", &self.previous)
            .field("connection", &self.connection)
            .finish_non_exhaustive()
    }
}

impl Prepared {
    /// Prospective owner, recorded by the runner before it confirms.
    #[must_use]
    pub const fn connection(&self) -> &Uuid7 { &self.connection }

    /// Owner observed while checking the opening; confirmation compares it again.
    #[must_use]
    pub const fn previous(&self) -> Option<&Uuid7> { self.previous.as_ref() }
}

#[derive(sqlx::FromRow)]
struct Preparatory {
    connection_id: Option<Uuid7>,
    credential_matches: bool,
    active: bool,
    can_resume: bool,
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
    pub(crate) const fn digest(&self) -> &Digest { &self.digest }

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
        match self.prepare_connection(runner, digest, previous, None, now).await? {
            Decision::Allowed(prepared) => self.confirm_connection(prepared).await,
            Decision::Refused(refusal) => Ok(Decision::Refused(refusal)),
        }
    }

    /// Inspect ownership without mutating it, then propose one new physical owner.
    /// `unconfirmed` is the proposal whose acknowledgement the runner may have lost.
    ///
    /// # Errors
    /// Preserves database and entropy failures; no ownership changes on this read.
    pub async fn prepare_connection(
        &self,
        runner: &Uuid7,
        digest: Digest,
        previous: Option<&Uuid7>,
        unconfirmed: Option<&Uuid7>,
        now: Timestamp,
    ) -> Result<Decision<Prepared>> {
        let row = sqlx::query_as::<_, Preparatory>(sql::PREPARE)
            .bind(runner)
            .bind(digest.as_str())
            .bind(crate::sql::RUNNER_STATUS_ACTIVE)
            .bind(previous)
            .bind(unconfirmed)
            .bind(previous.is_none() && unconfirmed.is_none())
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(match row {
            None | Some(Preparatory { credential_matches: false, .. }) => Decision::Refused(Refusal::Credential),
            Some(Preparatory { active: false, .. }) => Decision::Refused(Refusal::Revoked),
            Some(Preparatory { can_resume: false, .. }) => Decision::Refused(Refusal::Superseded),
            Some(row) => Decision::Allowed(Prepared {
                runner: runner.clone(),
                previous: row.connection_id,
                connection: self.entropy().uuid7(now)?,
                digest,
            }),
        })
    }

    /// Commit a proposal only after its runner has acknowledged both possible owners.
    /// A concurrent replacement makes this proof stale instead of stealing ownership back.
    ///
    /// # Errors
    /// Preserves database failures; durable refusals remain classified decisions.
    pub async fn confirm_connection(&self, prepared: Prepared) -> Result<Decision<Authority>> {
        let row = sqlx::query_as::<_, Checked>(sql::ACQUIRE)
            .bind(&prepared.runner)
            .bind(prepared.digest.as_str())
            .bind(&prepared.connection)
            .bind(crate::sql::RUNNER_STATUS_ACTIVE)
            .bind(prepared.previous.as_ref())
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(match row.map(Checked::decision) {
            Some(Decision::Allowed(())) => Decision::Allowed(Authority {
                runner: prepared.runner,
                connection: prepared.connection,
                digest: prepared.digest,
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
