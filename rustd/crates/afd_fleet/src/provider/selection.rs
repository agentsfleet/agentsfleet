//! The two rows that say WHERE a provider key is, before anything is opened.
//!
//! Neither read touches the vault. That split is the reason resolution can say
//! "this tenant has no workspace" or "no platform default is configured"
//! without having decrypted anything — and it is why the strategy a tenant
//! resolves through is a value that exists before the key does.

use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, provider_malformed, query, row_malformed};
use crate::provider::store::Providers;
use crate::sql;
use afd_billing::Posture;

/// Statement name, for the context a query failure carries.
const CONTEXT_SELECTION: &str = "tenant model selection";

/// Statement name, for the context a query failure carries.
const CONTEXT_PLATFORM: &str = "active platform default";

/// Statement name, for the context a query failure carries.
const CONTEXT_WORKSPACE: &str = "tenant primary workspace";

/// The table a malformed identifier is reported against.
const TABLE_PLATFORM_DEFAULTS: &str = "core.platform_provider_defaults";

/// The table a malformed identifier is reported against.
const TABLE_WORKSPACES: &str = "core.workspaces";

/// The column a malformed identifier is reported against.
const COLUMN_SOURCE_WORKSPACE: &str = "source_workspace_id";

/// The column a malformed identifier is reported against.
const COLUMN_ID: &str = "id";

/// The `mode` column held a word neither posture spells.
const FIELD_MODE: &str = "mode";

/// The active platform row carried no model to price against.
const FIELD_PLATFORM_MODEL: &str = "model";

/// What a tenant configured for itself.
///
/// `secret_ref` is `None` under the platform posture and carries the vault key
/// name under the self-managed one. The column allows both for both, and the
/// strategy that needs it refuses at construction rather than trusting it — see
/// [`super::managed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Who supplies the provider key.
    pub posture: Posture,
    /// The provider named at the time the tenant configured it.
    ///
    /// Read but unused by resolution: under the platform posture the live
    /// default row supplies the provider, and under self-managed the CREDENTIAL
    /// does. It is carried because the statement is the tenant plane's too, and
    /// narrowing the projection here would fork one statement into two.
    pub provider: Box<str>,
    /// The catalogue model this tenant's runs are priced against.
    pub model: Box<str>,
    /// The context ceiling the engine is handed.
    pub context_cap_tokens: u32,
    /// The vault key name holding a self-managed credential.
    pub secret_ref: Option<Box<str>>,
}

/// The one active platform default, as an operator set it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformDefault {
    /// The provider, which is ALSO the vault key name its credential is held
    /// under — `resolvePlatformDefault` passes `plk.provider` as the key.
    pub provider: Box<str>,
    /// The admin workspace holding that credential.
    pub source_workspace_id: Uuid7,
    /// The priced catalogue model this default resolves to.
    pub model: Box<str>,
    /// A custom endpoint for a non-named default; `None` for a named provider,
    /// which dials a built-in host.
    pub base_url: Option<Box<str>>,
    /// The context ceiling the engine is handed.
    pub context_cap_tokens: u32,
}

impl Providers {
    /// What `tenant_id` configured, or nothing if it never has.
    ///
    /// `Ok(None)` is not a failure and not a default — it is a tenant who has
    /// never configured a provider, and it resolves through the platform
    /// default exactly as an explicit `platform` row does. The collapse is in
    /// [`super::Providers::strategy`], where both readings meet one arm.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a `mode` column holding
    /// a word neither posture spells — which is a data-integrity fault to
    /// surface rather than a value to guess at.
    pub(crate) async fn selection(&self, tenant_id: &Uuid7) -> Result<Option<Selection>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::provider::SELECT_TENANT_MODEL_SELECTION)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SELECTION))?
            .as_ref()
            .map(read_selection)
            .transpose()
    }

    /// The active platform default, or nothing if no operator has set one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a `source_workspace_id` that
    /// is not an identifier, and an active row carrying no model — which is a
    /// row that predates a proper default-set and must be re-set through the
    /// dashboard rather than resolved to an unpriced model.
    pub(crate) async fn platform_default(&self) -> Result<Option<PlatformDefault>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query(sql::provider::SELECT_ACTIVE_PLATFORM_DEFAULT)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_PLATFORM))?
            .as_ref()
            .map(read_platform_default)
            .transpose()
    }

    /// The workspace `tenant_id` holds its self-managed credentials in.
    ///
    /// `Ok(None)` is a tenant with no workspace at all — a violated bootstrap
    /// invariant, since signup creates the primary workspace, and permanent.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an `id` column that is
    /// not an identifier.
    pub(crate) async fn primary_workspace(&self, tenant_id: &Uuid7) -> Result<Option<Uuid7>> {
        let mut connection = self.pool().acquire().await?;
        let found: Option<String> = sqlx::query_scalar(sql::provider::SELECT_PRIMARY_WORKSPACE)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_WORKSPACE))?;

        found
            .map(|id| Uuid7::parse(&id).map_err(row_malformed(TABLE_WORKSPACES, COLUMN_ID)))
            .transpose()
    }
}

/// One `core.tenant_model_selection` row, typed.
///
/// Not a [`sqlx::FromRow`], and the reason is the `mode` column: a word neither
/// posture spells is a PERMANENT configuration fault, and `FromRow` can only
/// answer `sqlx::Error`, which this crate reports as a datastore failure and
/// therefore retries. The classification is the point, so the conversion is
/// written where it can produce the right kind.
fn read_selection(row: &PgRow) -> Result<Selection> {
    let stored: String = row.try_get(0).map_err(query(CONTEXT_SELECTION))?;
    let posture = Posture::parse(&stored).ok_or_else(|| provider_malformed(FIELD_MODE))?;
    Ok(Selection {
        posture,
        provider: text(row, 1, CONTEXT_SELECTION)?,
        model: text(row, 2, CONTEXT_SELECTION)?,
        context_cap_tokens: cap(row.try_get(3).map_err(query(CONTEXT_SELECTION))?),
        secret_ref: optional_text(row, 4, CONTEXT_SELECTION)?,
    })
}

/// One `core.platform_provider_defaults` row, typed.
fn read_platform_default(row: &PgRow) -> Result<PlatformDefault> {
    // A NULL model means the active row predates a proper default-set. It fails
    // like a MISSING key rather than resolving: an unpriced model would bill
    // nothing and look like a working configuration.
    let model = optional_text(row, 2, CONTEXT_PLATFORM)?
        .ok_or_else(|| provider_malformed(FIELD_PLATFORM_MODEL))?;
    let workspace: String = row.try_get(1).map_err(query(CONTEXT_PLATFORM))?;
    let stored_cap: Option<i32> = row.try_get(4).map_err(query(CONTEXT_PLATFORM))?;

    Ok(PlatformDefault {
        provider: text(row, 0, CONTEXT_PLATFORM)?,
        source_workspace_id: Uuid7::parse(&workspace).map_err(row_malformed(
            TABLE_PLATFORM_DEFAULTS,
            COLUMN_SOURCE_WORKSPACE,
        ))?,
        model,
        base_url: optional_text(row, 3, CONTEXT_PLATFORM)?,
        context_cap_tokens: cap(stored_cap.unwrap_or_default()),
    })
}

/// One `text` column, owned.
fn text(row: &PgRow, index: usize, context: &'static str) -> Result<Box<str>> {
    row.try_get::<String, _>(index)
        .map(String::into_boxed_str)
        .map_err(query(context))
}

/// One nullable `text` column, owned.
fn optional_text(row: &PgRow, index: usize, context: &'static str) -> Result<Option<Box<str>>> {
    row.try_get::<Option<String>, _>(index)
        .map(|held| held.map(String::into_boxed_str))
        .map_err(query(context))
}

/// A stored context ceiling, clamped at zero.
///
/// The column is `int4` and the ceiling is a count, so a negative is a value
/// the schema permits and the domain does not. Clamping matches
/// `@intCast(@max(cap_i32, 0))` and, more to the point, keeps a corrupt row
/// from becoming a four-billion-token ceiling by wrapping.
fn cap(stored: i32) -> u32 {
    u32::try_from(stored).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::cap;

    #[test]
    fn a_negative_context_ceiling_clamps_to_zero_rather_than_wrapping() {
        assert_eq!(cap(0), 0);
        assert_eq!(cap(200_000), 200_000);
        assert_eq!(cap(-1), 0);
        assert_eq!(cap(i32::MIN), 0);
        assert_eq!(cap(i32::MAX), 2_147_483_647);
    }
}
