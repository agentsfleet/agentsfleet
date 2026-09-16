//! The three credential directories, over one pool.
//!
//! # Why these live here and not in `afd_auth`
//!
//! `src/auth/` in the Zig tree may not import `src/db/`, and `make test-auth`
//! greps to keep it that way; `afd_auth` reaches the same wall by construction,
//! because it does not list `sqlx` and so cannot name it. The concrete lookups
//! therefore live with the host, and this crate is where they land.
//!
//! # One implementation, three statements
//!
//! Zig wires three separate `LookupFn` pointers from one `serve_boot`, each
//! carrying its own `Ctx` struct holding the same pool. Here it is one type
//! holding one pool and one `resolve` that dispatches on the class, because the
//! class is already an enum the caller has in hand — the plumbing was three
//! times as wide to say the same thing.
//!
//! # The distinction the whole trait exists for
//!
//! `Ok(None)` and `Err(Unavailable)` must never collapse into each other. A
//! digest that matches nothing is an authentication REJECTION; a Postgres blip
//! is not. The runner client counts consecutive rejections toward a
//! self-termination ceiling and resets that counter on transport-class
//! failures, so answering an outage with a rejection walks a healthy fleet's
//! runners to shutdown. Every `?` in this file is chosen against that rule.

mod rows;

use crate::error::{Result, Unavailable};
use afd_auth::credential::CredentialKind;
use afd_auth::directory::{CredentialDirectory, CredentialRecord, Digest};
use afd_core::error_code;
use afd_db::Db;
use sqlx::FromRow;
use sqlx::postgres::PgRow;

use self::rows::{CliCredentialRow, RunnerTokenRow, TenantApiKeyRow};
use crate::sql;

/// Resolves credential digests against Postgres.
///
/// Holds the api-role pool: every lookup here is on the request path, and a
/// request-path read that shares a pool with background work waits behind it.
#[derive(Debug, Clone)]
pub struct Credentials {
    database: Db,
}

impl Credentials {
    /// A directory reading through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Runs `statement` for `digest`, returning the row it matched.
    ///
    /// Every datastore failure — a pool with nothing to give, a connection that
    /// dropped, a statement Postgres refused, a column the row did not have —
    /// becomes [`Unavailable`] here, through ONE path. A query that simply
    /// matched nothing becomes `Ok(None)` by being a `None` row rather than an
    /// error, and keeping those two apart is what this whole trait is for.
    async fn fetch<R>(
        &self,
        class: &'static str,
        statement: &'static str,
        digest: &Digest,
    ) -> Result<Option<R>>
    where
        R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    {
        let mut connection = self
            .database
            .acquire()
            .await
            .inspect_err(|failure| {
                let code = error_code::INTERNAL_DB_UNAVAILABLE.as_str();
                let reason = failure.to_string();
                tracing::warn!(
                    error_code = code,
                    class,
                    reason,
                    event = "credential_lookup_unavailable",
                    "no connection for a credential lookup — answering unavailable, \
                 never unknown, so a caller is not told its credential is bad"
                );
            })
            // `inspect_err` observes, `map_err` maps: this error carries no source,
            // so stringifying into it would have dropped the chain (RULE ERR-RS).
            .map_err(|_logged| Unavailable)?;

        sqlx::query_as::<_, R>(statement)
            .bind(digest.as_str())
            .fetch_optional(&mut *connection)
            .await
            .inspect_err(|failure| {
                let code = error_code::INTERNAL_DB_QUERY.as_str();
                let reason = failure.to_string();
                tracing::warn!(
                    error_code = code,
                    class,
                    reason,
                    event = "credential_lookup_failed",
                    "credential lookup failed"
                );
            })
            // `inspect_err` observes, `map_err` maps: the cause reaches the log
            // and never the error VALUE, which carries no source (RULE ERR-RS).
            .map_err(|_logged| Unavailable)
    }

    /// The record `digest` resolves to under `R`, or `None` if it matched no row.
    ///
    /// One body for all three classes: [`Resolved`] holds the four facts they
    /// differ in, so a miss stays `Ok(None)` and a mapping failure stays `Err`
    /// through a single `transpose` rather than three `let … else` returns that
    /// each had their own chance to collapse the two.
    async fn lookup<R: Resolved>(&self, digest: &Digest) -> Result<Option<CredentialRecord>> {
        self.fetch::<R>(R::CLASS, R::STATEMENT, digest)
            .await?
            .map(R::into_record)
            .transpose()
    }
}

/// A credential row that knows how it is looked up and what it becomes.
///
/// The three lookups differed in exactly four facts — the class label, the
/// statement, the row type and the record it reads into — and spelled them in
/// three bodies carrying the same `let Some(row) = … else { return Ok(None) }`.
/// Held as associated items, that shape is written once in
/// [`Credentials::lookup`] and a fourth credential class becomes an `impl`
/// rather than a fourth copy of the shape.
trait Resolved: for<'r> FromRow<'r, PgRow> + Send + Unpin {
    /// The class label, as this lookup's log lines spell it.
    const CLASS: &'static str;
    /// The statement selecting the row by digest.
    const STATEMENT: &'static str;

    /// Reads the row into the record a resolver answers with.
    fn into_record(self) -> Result<CredentialRecord>;
}

/// `agt_t` — the key's row, and the person who minted it.
impl Resolved for TenantApiKeyRow {
    const CLASS: &'static str = "tenant_api_key";
    const STATEMENT: &'static str = sql::SELECT_TENANT_API_KEY;

    fn into_record(self) -> Result<CredentialRecord> {
        rows::person(&self.tenant, &self.subject, self.live)
    }
}

/// `afc_` — the credential's row, joined to the person who holds it.
impl Resolved for CliCredentialRow {
    const CLASS: &'static str = "cli_credential";
    const STATEMENT: &'static str = sql::SELECT_CLI_CREDENTIAL;

    fn into_record(self) -> Result<CredentialRecord> {
        rows::person(&self.tenant, &self.subject, self.live)
    }
}

/// `agt_r` — the runner's row, with its reconciled verdict.
impl Resolved for RunnerTokenRow {
    const CLASS: &'static str = "runner_token";
    const STATEMENT: &'static str = sql::SELECT_RUNNER_TOKEN;

    fn into_record(self) -> Result<CredentialRecord> {
        rows::machine(&self.runner, self.degraded, self.live)
    }
}

impl CredentialDirectory for Credentials {
    async fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> Result<Option<CredentialRecord>> {
        match kind {
            CredentialKind::TenantApiKey => self.lookup::<TenantApiKeyRow>(digest).await,
            CredentialKind::CliCredential => self.lookup::<CliCredentialRow>(digest).await,
            CredentialKind::RunnerToken => self.lookup::<RunnerTokenRow>(digest).await,
            // Never asked for: a session token is verified, not looked up, and
            // the caller proves that by dispatch. `Ok(None)` is what the trait
            // asks an implementation to answer if it is asked anyway — there is
            // no store to consult, so there is nothing here to be unavailable.
            CredentialKind::OidcSessionToken => Ok(None),
        }
    }
}
