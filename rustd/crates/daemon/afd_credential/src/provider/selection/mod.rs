//! The two rows that say WHERE a provider key is, before anything is opened.
//!
//! Neither read touches the vault's ciphertext. That split is the reason
//! resolution can say "this tenant has no workspace" or "no platform default
//! is configured" without having decrypted anything — and it is why the
//! strategy a tenant resolves through is a value that exists before the key
//! does.

use afd_core::id::Uuid7;

use crate::error::{Result, provider_malformed, query, row_malformed};
use crate::provider::cap;
use crate::provider::endpoint::OPENAI_COMPATIBLE;
use crate::provider::sql;
use crate::provider::store::Providers;
use afd_billing::Posture;

/// Statement name, for the context a query failure carries.
const CONTEXT_SELECTION: &str = "tenant model selection";

/// Statement name, for the context a query failure carries.
const CONTEXT_PLATFORM: &str = "active platform default";

/// Statement name, for the context a query failure carries.
const CONTEXT_WORKSPACE: &str = "tenant primary workspace";

/// The table a malformed identifier is reported against.
const TABLE_PLATFORM_DEFAULTS: &str = "model.platform_defaults";

/// The table a malformed identifier is reported against.
const TABLE_SELECTIONS: &str = "model.tenant_selections";

/// The table a malformed identifier is reported against.
///
/// `pub(crate)` because the activation reads the same bridge row off its own
/// locking statement and must report a malformed id against the same table
/// (RULE UFS: one spelling, not two).
pub(crate) const TABLE_WORKSPACES: &str = "identity.workspaces";

/// The column a malformed identifier is reported against.
const COLUMN_SOURCE_WORKSPACE: &str = "source_workspace_id";

/// The column a malformed identifier is reported against.
pub(crate) const COLUMN_TENANT_MODEL: &str = "tenant_model_id";

/// The column a malformed identifier is reported against.
pub(crate) const COLUMN_ID: &str = "id";

/// A selection pointing at a registry row that came back without its model.
const FIELD_MODEL_NAME: &str = "model_name";

/// The active platform row carried no model to price against.
const FIELD_PLATFORM_MODEL: &str = "model";

/// What a tenant configured for itself.
///
/// Two states, and the type holds no third: the platform default names no
/// credential, and a self-managed selection always names one. The column
/// behind it is one nullable pointer, so there is no stored word to misspell
/// and no posture/credential pairing to contradict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// The platform default, chosen explicitly: a row with a NULL pointer.
    Platform,
    /// One of the tenant's own registry entries.
    SelfManaged(OwnModel),
}

impl Selection {
    /// Who supplies the provider key under this selection.
    #[must_use]
    pub const fn posture(&self) -> Posture {
        match self {
            Self::Platform => Posture::Platform,
            Self::SelfManaged(_) => Posture::SelfManaged,
        }
    }
}

/// The registry entry a self-managed selection points at, as it resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnModel {
    /// The `model.tenant_models` row the selection points at.
    pub tenant_model_id: Uuid7,
    /// The model this tenant's runs are priced against.
    pub model: Box<str>,
    /// The vault key name holding the credential.
    pub secret_ref: Box<str>,
    /// The provider the credential's projection names.
    ///
    /// `None` when the credential was deleted out of band or names no
    /// provider. Read but unused by resolution — the decrypted CREDENTIAL
    /// supplies the provider there — and carried for the tenant plane, which
    /// renders it.
    pub provider: Option<Box<str>>,
    /// The context ceiling the engine is handed, read live from the catalogue.
    pub context_cap_tokens: u32,
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

/// One [`sql::SELECT_TENANT_SELECTION`] row, by the names it gives its columns.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SelectionRow {
    tenant_model_id: Option<String>,
    model_name: Option<String>,
    secret_ref: Option<String>,
    provider: Option<String>,
    context_cap_tokens: i32,
}

impl SelectionRow {
    /// The row, typed.
    ///
    /// A NULL pointer is the platform posture. A pointer whose joined entry
    /// came back without its model or credential name is a row the composite
    /// foreign key should have made impossible, so it is a PERMANENT
    /// configuration fault rather than a value to guess at.
    ///
    /// # Errors
    /// Reports a pointer that is not an identifier, and a pointer naming no
    /// readable entry.
    pub(crate) fn into_selection(self) -> Result<Selection> {
        let Some(pointer) = self.tenant_model_id else {
            return Ok(Selection::Platform);
        };
        let tenant_model_id =
            Uuid7::parse(&pointer).map_err(row_malformed(TABLE_SELECTIONS, COLUMN_TENANT_MODEL))?;
        let (Some(model), Some(secret_ref)) = (self.model_name, self.secret_ref) else {
            return Err(provider_malformed(FIELD_MODEL_NAME));
        };
        Ok(Selection::SelfManaged(OwnModel {
            tenant_model_id,
            model: model.into_boxed_str(),
            secret_ref: secret_ref.into_boxed_str(),
            provider: self.provider.map(String::into_boxed_str),
            context_cap_tokens: cap::stored(self.context_cap_tokens),
        }))
    }
}

/// One [`sql::SELECT_ACTIVE_PLATFORM_DEFAULT`] row, by its column names.
#[derive(Debug, sqlx::FromRow)]
struct PlatformDefaultRow {
    provider: String,
    source_workspace_id: String,
    model_name: Option<String>,
    base_url: Option<String>,
    context_cap_tokens: Option<i32>,
}

impl PlatformDefaultRow {
    /// The row, typed.
    ///
    /// Deactivation clears the model, and this read takes active rows only, so
    /// a NULL model here is a corrupt row. It fails like a MISSING key rather
    /// than resolving: an unpriced model would bill nothing and look like a
    /// working configuration.
    fn into_default(self) -> Result<PlatformDefault> {
        let model = self
            .model_name
            .ok_or_else(|| provider_malformed(FIELD_PLATFORM_MODEL))?;
        Ok(PlatformDefault {
            provider: self.provider.into_boxed_str(),
            source_workspace_id: Uuid7::parse(&self.source_workspace_id).map_err(row_malformed(
                TABLE_PLATFORM_DEFAULTS,
                COLUMN_SOURCE_WORKSPACE,
            ))?,
            model: model.into_boxed_str(),
            base_url: self.base_url.map(String::into_boxed_str),
            context_cap_tokens: cap::stored(self.context_cap_tokens.unwrap_or_default()),
        })
    }
}

impl Providers {
    /// What `tenant_id` configured, or nothing if it never has.
    ///
    /// `Ok(None)` is not a failure and not a default — it is a tenant who has
    /// never configured a provider, and it resolves through the platform
    /// default exactly as [`Selection::Platform`] does. The collapse is in
    /// [`super::Providers::strategy`], where both readings meet one arm.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a pointer naming no
    /// readable registry entry — a data-integrity fault to surface rather than
    /// a value to guess at.
    pub async fn selection(&self, tenant_id: &Uuid7) -> Result<Option<Selection>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as::<_, SelectionRow>(sql::SELECT_TENANT_SELECTION)
            .bind(tenant_id.as_str())
            .bind(OPENAI_COMPATIBLE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SELECTION))?
            .map(SelectionRow::into_selection)
            .transpose()
    }

    /// The active platform default, or nothing if no operator has set one.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a `source_workspace_id` that
    /// is not an identifier, and an active row carrying no model. Only
    /// deactivation clears the model, so an active row without one is corrupt
    /// and must be re-set through the dashboard rather than resolved to an
    /// unpriced model.
    pub async fn platform_default(&self) -> Result<Option<PlatformDefault>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as::<_, PlatformDefaultRow>(sql::SELECT_ACTIVE_PLATFORM_DEFAULT)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_PLATFORM))?
            .map(PlatformDefaultRow::into_default)
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
        let found: Option<String> = sqlx::query_scalar(sql::SELECT_PRIMARY_WORKSPACE)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_WORKSPACE))?;

        found
            .map(|id| Uuid7::parse(&id).map_err(row_malformed(TABLE_WORKSPACES, COLUMN_ID)))
            .transpose()
    }
}

#[cfg(test)]
mod tests;
