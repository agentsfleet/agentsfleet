//! Whose workspace this is — the ownership half of authorization.
//!
//! # Why this is a service and not a helper each handler calls
//!
//! `authorizeWorkspace` is a Zig function called by hand at the top of every
//! workspace handler. That is the shape this crate exists to break: a rule
//! enforced by remembering to call something is a rule with one exception per
//! author. What lives here is the DECISION — one statement, one verdict, no
//! HTTP — and `afd_api` mounts it as a layer in front of every route whose
//! template carries a workspace, so no handler is in a position to forget it.
//!
//! # `Ok(None)` is not `Err`
//!
//! A workspace that is not the caller's answers `Ok(None)`; a pool with nothing
//! to give answers `Err`. Collapsing them would tell a tenant their own
//! workspace had vanished during a Postgres blip, and a dashboard acting on
//! that would show a person their work was gone (RULE ECL). This is the
//! `Result<Option<T>>` convention `core_api` runs on, and the reason it is the
//! convention.

use afd_auth::principal::{Person, PersonCredential, Principal};
use afd_auth::scope::Scope;
use afd_core::id::Uuid7;
use afd_db::Db;

use crate::sql::workspace as sql;
use crate::{Result, error};

/// Resolves who owns a workspace.
///
/// Holds the api-role pool: this read is on the request path of every workspace
/// route, and a request-path read sharing a pool with background work waits
/// behind it.
#[derive(Debug, Clone)]
pub struct Workspaces {
    database: Db,
}

impl Workspaces {
    /// A resolver reading through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// The tenant owning `workspace`, when this principal's does.
    ///
    /// # The ordering is load-bearing
    ///
    /// The session token's workspace ceiling is checked BEFORE the statement.
    /// It is a claim already in hand, so a scoped principal reaching for a
    /// workspace outside its ceiling costs no round trip at all — and, more to
    /// the point, cannot reach a datastore on the strength of a claim that
    /// already refuses it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A workspace that is not this
    /// caller's is `Ok(None)`, never an error — see the module note.
    pub async fn authorize(
        &self,
        principal: &Principal,
        workspace: &Uuid7,
    ) -> Result<Option<Uuid7>> {
        let Some(person) = principal.person() else {
            // A runner has no tenant authority at all, so the statement could
            // never match. Refused without a round trip rather than by asking a
            // question whose answer is already known.
            return Ok(None);
        };
        if let Some(ceiling) = person.workspace_scope()
            && ceiling != workspace
        {
            return Ok(None);
        }

        if let Some(tenant) = self.owner_matching(person, workspace).await? {
            return Ok(Some(tenant));
        }
        self.cross_tenant_override(principal, person, workspace)
            .await
    }

    /// The owning tenant, when it is the one this principal resolves to.
    async fn owner_matching(&self, person: &Person, workspace: &Uuid7) -> Result<Option<Uuid7>> {
        let binds = TenantBinds::of(person);
        let mut connection = self.database.acquire().await?;
        let row: Option<(String,)> = sqlx::query_as(sql::AUTHORIZE_WORKSPACE)
            .bind(workspace.as_str())
            .bind(binds.subject)
            .bind(binds.claim)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("authorize workspace"))?;
        row.map(|(tenant,)| parse_tenant(&tenant)).transpose()
    }

    /// The audited platform-wide override, for the few principals holding it.
    ///
    /// Engages ONLY after the tenant-scoped check has already denied, and only
    /// for a principal holding the platform-wide workspace scope. Every use is
    /// recorded before it is honoured, because this is the sole path by which
    /// one tenant's operator reaches another tenant's workspace and an
    /// unrecorded one would be indistinguishable from the cross-tenant read
    /// this whole layer exists to stop.
    async fn cross_tenant_override(
        &self,
        principal: &Principal,
        person: &Person,
        workspace: &Uuid7,
    ) -> Result<Option<Uuid7>> {
        if !principal.scopes().contains(Scope::WorkspaceAny) {
            return Ok(None);
        }
        let mut connection = self.database.acquire().await?;
        let row: Option<(String,)> = sqlx::query_as(sql::SELECT_WORKSPACE_TENANT)
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("resolve workspace tenant"))?;
        let Some((tenant,)) = row else {
            return Ok(None);
        };
        let tenant = parse_tenant(&tenant)?;

        // Emitted BEFORE the override is honoured, so a crash between the
        // decision and the work still leaves the record. Hoisted fields: the
        // `log` bridge duplicates every expression and llvm-cov scores the
        // dead copy.
        let subject = person.subject().as_str();
        let acting_tenant = person.tenant().as_str();
        let target_tenant = tenant.as_str();
        let target_workspace = workspace.as_str();
        // `warn`, and it is the one refusal-adjacent event in this file that
        // earns it: an operator crossing a tenant boundary is rare, legitimate,
        // and exactly what somebody reviewing an incident needs to find.
        tracing::warn!(
            subject,
            acting_tenant,
            target_tenant,
            target_workspace,
            event = "cross_tenant_workspace_override",
            "a platform-scoped principal reached another tenant's workspace"
        );
        Ok(Some(tenant))
    }

    /// The tenant a subject belongs to, with no workspace to check against.
    ///
    /// The cold path — creating a workspace, and the tenant-scoped lists that
    /// carry no workspace identifier. A claim-bound credential resolved its
    /// tenant at authentication time and its principal already carries the
    /// answer, so only a browser session reaches the statement.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn tenant_of(&self, principal: &Principal) -> Result<Option<Uuid7>> {
        let Some(person) = principal.person() else {
            return Ok(None);
        };
        match person.credential() {
            PersonCredential::TenantApiKey | PersonCredential::CliCredential => {
                return Ok(Some(person.tenant().clone()));
            }
            PersonCredential::SessionToken { .. } => {}
        }

        let mut connection = self.database.acquire().await?;
        let row: Option<(String,)> = sqlx::query_as(sql::SELECT_USER_TENANT_BY_SUBJECT)
            .bind(person.subject().as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("resolve subject tenant"))?;
        match row {
            Some((tenant,)) => parse_tenant(&tenant).map(Some),
            // The claim stands when no user row exists, which is the same
            // fallback the `COALESCE` above encodes.
            None => Ok(Some(person.tenant().clone())),
        }
    }
}

/// The two binds a merged tenant-resolving statement needs.
///
/// A struct rather than a pair of `Option<&str>`, because both are optional
/// strings and transposing them would silently authorize against the wrong
/// authority — the subject arm outranks the claim arm, so the swap denies
/// legitimate callers and, worse, would admit a claim the user row was meant
/// to override.
#[derive(Debug, Clone, Copy)]
struct TenantBinds<'a> {
    /// The identity provider's subject, for the user-row arm.
    subject: Option<&'a str>,
    /// The token's tenant claim, for the fallback arm.
    claim: Option<&'a str>,
}

impl<'a> TenantBinds<'a> {
    /// What this person binds.
    ///
    /// Never empty, unlike the Zig `principalTenantBinds` it replaces: that one
    /// answers null for a runner so its callers can deny without a round trip,
    /// and here a runner never reaches this function at all — it was refused one
    /// frame up, by not being a `Person`. The type says so, so there is no arm.
    fn of(person: &'a Person) -> Self {
        // Only a browser session binds the subject. A claim-bound credential
        // resolved its tenant through the user row at authentication time, so
        // re-reading it here would be a second round trip for a value the
        // principal already carries — and its claim is therefore authoritative.
        let subject = match person.credential() {
            PersonCredential::SessionToken { .. } => Some(person.subject().as_str()),
            PersonCredential::TenantApiKey | PersonCredential::CliCredential => None,
        };
        Self {
            subject,
            claim: Some(person.tenant().as_str()),
        }
    }
}

/// A stored tenant identifier, or a report that the column holds something else.
fn parse_tenant(value: &str) -> Result<Uuid7> {
    Uuid7::parse(value).map_err(error::row_malformed("core.workspaces", "tenant_id"))
}
