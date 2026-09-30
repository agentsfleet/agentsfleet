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

pub mod access;
pub mod accounts;
pub mod crossing;
pub mod directory;
pub mod name;

use afd_auth::principal::{Person, PersonCredential, Principal};
use afd_auth::scope::Scope;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Row as _;

use self::access::{Access, Grant, Role};
use crate::sql::workspace as sql;
use crate::{Result, error};

/// The context an access read's failure reports under.
const CONTEXT_AUTHORIZE: &str = "authorize workspace";

/// Resolves who owns a workspace, and keeps the tenant's directory of them.
///
/// Holds the api-role pool: the ownership read is on the request path of every
/// workspace route, and a request-path read sharing a pool with background
/// work waits behind it. The entropy source is the directory half's — it draws
/// identifiers and generated names at create — and lives here because one
/// value serves both halves of one type.
#[derive(Debug, Clone)]
pub struct Workspaces {
    database: Db,
    entropy: Entropy,
}

impl Workspaces {
    /// A resolver and directory over `database`, drawing from `entropy`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// The owning tenant and the caller's grant, when this principal may open
    /// `workspace`.
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
    ) -> Result<Option<Access>> {
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

        if let Some(access) = self.membership(person, workspace).await? {
            return Ok(Some(access));
        }
        self.cross_tenant_override(principal, workspace).await
    }

    /// The caller's grant from inside the owning account, when they hold one.
    ///
    /// A row with no stored role is the caller's own account admitted by the
    /// tenant match alone, which is exactly the rule this replaced, so it
    /// answers `owner`.
    async fn membership(&self, person: &Person, workspace: &Uuid7) -> Result<Option<Access>> {
        let binds = TenantBinds::of(person);
        let mut connection = self.database.acquire().await?;
        let unreadable = error::query(CONTEXT_AUTHORIZE);
        let row = sqlx::query(sql::AUTHORIZE_WORKSPACE)
            .bind(workspace.as_str())
            .bind(binds.subject)
            .bind(binds.claim)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(&unreadable)?;
        row.map(|row| {
            let tenant: String = row.try_get("tenant_id").map_err(&unreadable)?;
            let role: Option<String> = row.try_get("role").map_err(&unreadable)?;
            Ok(Access {
                tenant: parse_tenant(&tenant)?,
                grant: Grant::Membership(Role::held(role.as_deref())?),
            })
        })
        .transpose()
    }

    /// The platform-wide override, for the few principals holding it.
    ///
    /// Engages ONLY after the tenant-scoped check has already denied, and only
    /// for a principal holding the platform-wide workspace scope. This is the
    /// sole path by which one tenant's operator reaches another tenant's
    /// workspace, so every use is recorded before it is honoured — by the
    /// caller that honours it, through [`crossing::audit`], because only that
    /// caller knows the method and an open stream re-asks this on every beat.
    async fn cross_tenant_override(
        &self,
        principal: &Principal,
        workspace: &Uuid7,
    ) -> Result<Option<Access>> {
        if !principal.scopes().contains(Scope::WorkspaceAny) {
            return Ok(None);
        }
        let mut connection = self.database.acquire().await?;
        let tenant: Option<String> = sqlx::query_scalar(sql::SELECT_WORKSPACE_TENANT)
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query("resolve workspace tenant"))?;
        tenant
            .map(|tenant| {
                Ok(Access {
                    tenant: parse_tenant(&tenant)?,
                    grant: Grant::Platform,
                })
            })
            .transpose()
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
///
/// Every tenant identifier this module reads is a `core.tenants.id`, whichever
/// table carried it here, so that is the column a malformed one is reported as.
fn parse_tenant(value: &str) -> Result<Uuid7> {
    Uuid7::parse(value).map_err(error::row_malformed("core.tenants", "id"))
}
