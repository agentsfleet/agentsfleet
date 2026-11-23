//! The two rows that say WHERE a provider key is, before anything is opened.
//!
//! Neither read touches the vault's ciphertext. That split is the reason a
//! caller can say "this tenant has no workspace" or "no platform default is
//! configured" without having decrypted anything.

use af_core::id::Uuid7;

use crate::error::{Result, provider_malformed, query};
use crate::provider::cap;
use crate::provider::sql;
use crate::provider::store::Providers;
use afd_billing::Posture;

/// Statement name, for the context a query failure carries.
const CONTEXT_SELECTION: &str = "tenant model selection";

/// Statement name, for the context a query failure carries.
const CONTEXT_PLATFORM: &str = "active platform default";

/// The active platform row carried no model to price against.
const FIELD_PLATFORM_MODEL: &str = "model";

/// What a tenant configured for itself.
///
/// Two states, and the type holds no third: the platform default names no
/// credential, and a self-managed selection always names one. The column
/// behind it is one flag on the tenant's own rows, so there is no stored word
/// to misspell and no posture/credential pairing to contradict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// The platform default: none of the tenant's rows is selected.
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
    /// The selected `model.tenant_models` row.
    pub tenant_model_id: Uuid7,
    /// The model this tenant's runs are priced against.
    pub model: Box<str>,
    /// The vault key name holding the credential.
    pub secret_ref: Box<str>,
    /// The stored credential namespace, absent when its workspace is unavailable.
    pub workspace_id: Option<Uuid7>,
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
    /// under.
    pub provider: Box<str>,
    /// The priced catalogue model this default resolves to.
    pub model: Box<str>,
    /// The context ceiling the engine is handed.
    pub context_cap_tokens: u32,
}

/// One [`sql::SELECT_TENANT_SELECTION`] row, by the names it gives its columns.
#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SelectionRow {
    tenant_model_id: Uuid7,
    model_name: String,
    secret_ref: String,
    workspace_id: Option<Uuid7>,
    provider: Option<String>,
    context_cap_tokens: i32,
}

impl SelectionRow {
    /// The selected row, typed: always the self-managed posture, since the
    /// platform default is the absence of a selected row.
    pub(crate) fn into_selection(self) -> Selection {
        Selection::SelfManaged(OwnModel {
            tenant_model_id: self.tenant_model_id,
            model: self.model_name.into_boxed_str(),
            secret_ref: self.secret_ref.into_boxed_str(),
            workspace_id: self.workspace_id,
            provider: self.provider.map(String::into_boxed_str),
            context_cap_tokens: cap::stored(self.context_cap_tokens),
        })
    }
}

/// One [`sql::SELECT_ACTIVE_PLATFORM_DEFAULT`] row, by its column names.
#[derive(Debug, sqlx::FromRow)]
struct PlatformDefaultRow {
    provider: String,
    model_name: Option<String>,
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
            model: model.into_boxed_str(),
            context_cap_tokens: cap::stored(self.context_cap_tokens.unwrap_or_default()),
        })
    }
}

impl Providers {
    /// What `tenant_id` runs on: its selected model, or the platform default
    /// when none of its rows is selected — a tenant that never chose and one
    /// that reset read the same.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn selection(&self, tenant_id: &Uuid7) -> Result<Selection> {
        let mut connection = self.pool().acquire().await?;
        Ok(
            sqlx::query_as::<_, SelectionRow>(sql::SELECT_TENANT_SELECTION)
                .bind(tenant_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query(CONTEXT_SELECTION))?
                .map_or(Selection::Platform, SelectionRow::into_selection),
        )
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
}

#[cfg(test)]
mod tests;
