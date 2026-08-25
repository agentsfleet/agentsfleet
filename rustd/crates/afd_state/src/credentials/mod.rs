//! The three credential directories, over one pool.
//!
//! # Why these live here and not in `afd_auth`
//!
//! `src/auth/` in the Zig tree may not import `src/db/`, and `make test-auth`
//! greps to keep it that way; `afd_auth` reaches the same wall by construction,
//! because it does not list `sqlx` and so cannot name it. The concrete lookups
//! therefore live with the host — `cmd/api_key_lookup.zig`,
//! `cmd/cli_credential_lookup.zig`, `cmd/serve_runner_lookup.zig` — and this
//! crate is where that lands for the port.
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
//! runners to shutdown — `runner_bearer.zig`'s own test pins that behaviour.
//! Every `?` in this file is chosen against that rule.

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
        let mut connection = self.database.acquire().await.map_err(|source| {
            let code = error_code::INTERNAL_DB_UNAVAILABLE.as_str();
            let reason = source.to_string();
            tracing::warn!(
                error_code = code,
                class,
                reason,
                "no connection for a credential lookup — answering unavailable, \
                 never unknown, so a caller is not told its credential is bad"
            );
            Unavailable
        })?;

        sqlx::query_as::<_, R>(statement)
            .bind(digest.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(|source| {
                let code = error_code::INTERNAL_DB_QUERY.as_str();
                let reason = source.to_string();
                tracing::warn!(error_code = code, class, reason, "credential lookup failed");
                Unavailable
            })
    }

    /// `agt_t` — the key's row, and the person who minted it.
    async fn tenant_api_key(&self, digest: &Digest) -> Result<Option<CredentialRecord>> {
        let Some(row) = self
            .fetch::<TenantApiKeyRow>("tenant_api_key", sql::SELECT_TENANT_API_KEY, digest)
            .await?
        else {
            return Ok(None);
        };
        rows::person(&row.tenant, &row.subject, row.live).map(Some)
    }

    /// `afc_` — the credential's row, joined to the person who holds it.
    async fn cli_credential(&self, digest: &Digest) -> Result<Option<CredentialRecord>> {
        let Some(row) = self
            .fetch::<CliCredentialRow>("cli_credential", sql::SELECT_CLI_CREDENTIAL, digest)
            .await?
        else {
            return Ok(None);
        };
        rows::person(&row.tenant, &row.subject, row.live).map(Some)
    }

    /// `agt_r` — the runner's row, with its reconciled verdict.
    async fn runner_token(&self, digest: &Digest) -> Result<Option<CredentialRecord>> {
        let Some(row) = self
            .fetch::<RunnerTokenRow>("runner_token", sql::SELECT_RUNNER_TOKEN, digest)
            .await?
        else {
            return Ok(None);
        };
        rows::machine(&row.runner, row.degraded, row.live).map(Some)
    }
}

impl CredentialDirectory for Credentials {
    async fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> Result<Option<CredentialRecord>> {
        match kind {
            CredentialKind::TenantApiKey => self.tenant_api_key(digest).await,
            CredentialKind::CliCredential => self.cli_credential(digest).await,
            CredentialKind::RunnerToken => self.runner_token(digest).await,
            // Never asked for: a session token is verified, not looked up, and
            // the caller proves that by dispatch. `Ok(None)` is what the trait
            // asks an implementation to answer if it is asked anyway — there is
            // no store to consult, so there is nothing here to be unavailable.
            CredentialKind::OidcSessionToken => Ok(None),
        }
    }
}
