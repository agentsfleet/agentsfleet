//! An explicit owner model reference, independent of the owner's runtime default.
//!
//! The caller supplies its connection so dispatch authorization and the pinned
//! model read can share one transaction. This read never opens a credential.

mod key;
mod sql;

use afd_core::id::Uuid7;
use afd_wire::inference::ModelInference;
use sqlx::types::Json;
use sqlx::{Connection as _, PgConnection};

use crate::error::{Result, query};

/// Stable reference to the deployment's currently configured platform default.
pub const PLATFORM_MODEL_REF: &str = "platform-default";
pub(super) const CONTEXT: &str = "resolve registered owner model";

/// A reference that routes: its non-secret facts, the credential that calls it,
/// and the host its saved settings dial.
pub(super) struct Registration {
    pub(super) model: RegisteredModel,
    pub(super) workspace_id: String,
    pub(super) key_name: String,
    pub(super) inference_host: Box<str>,
}

/// Non-secret facts selected by an explicit reference.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegisteredModel {
    /// Opaque tenant registry reference, or the explicit platform reference.
    pub reference: Box<str>,
    /// Provider whose approved transport must support this model.
    pub provider: Box<str>,
    /// Provider model identifier, never supplied separately by a runner.
    pub model: Box<str>,
    /// Optional endpoint restriction carried by the credential.
    #[serde(default)]
    pub base_url: Option<Box<str>>,
    /// Persisted non-secret settings, pinned by the catalogue generation.
    pub inference: Option<ModelInference>,
    /// Priced catalogue context window.
    pub context_window_tokens: i32,
    /// Fresh input nanos per million tokens.
    pub input_nanos_per_million: i64,
    /// Output nanos per million tokens.
    pub output_nanos_per_million: i64,
    /// The selected model/credential/catalogue generation.
    pub resolution_version: Box<str>,
    /// Whether provider costs are paid by the platform wallet.
    pub platform_managed: bool,
}

/// One [`sql::ENTRY`] or [`sql::PLATFORM`] row, by the names both give.
#[derive(Debug, sqlx::FromRow)]
struct RegisteredRow {
    provider: String,
    model: String,
    context_window_tokens: i32,
    input_nanos_per_million: i64,
    output_nanos_per_million: i64,
    resolution_version: String,
    workspace_id: String,
    key_name: String,
    base_url: Option<String>,
    inference: Option<Json<ModelInference>>,
}

impl RegisteredRow {
    /// The non-secret facts, as selected by `reference`.
    fn metadata(&self, reference: &str) -> RegisteredModel {
        RegisteredModel {
            reference: reference.into(),
            provider: self.provider.as_str().into(),
            model: self.model.as_str().into(),
            base_url: self.base_url.as_deref().map(Into::into),
            inference: self.inference.as_ref().map(|settings| settings.0.clone()),
            context_window_tokens: self.context_window_tokens,
            input_nanos_per_million: self.input_nanos_per_million,
            output_nanos_per_million: self.output_nanos_per_million,
            resolution_version: self.resolution_version.as_str().into(),
            platform_managed: reference == PLATFORM_MODEL_REF,
        }
    }
}

/// Resolves exactly `reference` for the workspace owner, without fallback.
///
/// # Errors
/// Reports a failed statement; missing credentials, entries, prices and
/// inference settings answer `None`. The reference is opaque even when today's
/// entries use UUIDs.
pub async fn registered_model(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RegisteredModel>> {
    Ok(read(connection, owner, reference)
        .await?
        .map(|registration| registration.model))
}

/// The routable registration for `reference`, or nothing when the row is
/// absent, carries no saved settings, or its settings do not route within the
/// model's context and the credential's endpoint.
pub(super) async fn read(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<Registration>> {
    // A savepoint inside the caller's transaction, a transaction of its own
    // on a bare connection: either way the locks below are taken in order and
    // held together. See `sql`'s lock order.
    let mut ordered = connection.begin().await.map_err(query(CONTEXT))?;
    sqlx::query(sql::LOCK_CATALOGUE)
        .execute(&mut *ordered)
        .await
        .map_err(query(CONTEXT))?;
    let (credential, statement) = if reference == PLATFORM_MODEL_REF {
        (sqlx::query(sql::LOCK_PLATFORM_CREDENTIAL), sql::PLATFORM)
    } else {
        let credential = sqlx::query(sql::LOCK_ENTRY_CREDENTIAL)
            .bind(owner.as_str())
            .bind(reference);
        (credential, sql::ENTRY)
    };
    credential
        .execute(&mut *ordered)
        .await
        .map_err(query(CONTEXT))?;
    let row = sqlx::query_as::<_, RegisteredRow>(statement)
        .bind(owner.as_str())
        .bind(reference)
        .fetch_optional(&mut *ordered)
        .await
        .map_err(query(CONTEXT))?;
    ordered.commit().await.map_err(query(CONTEXT))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let model = row.metadata(reference);
    let Some(inference) = &model.inference else {
        return Ok(None);
    };
    let context = u32::try_from(model.context_window_tokens).unwrap_or_default();
    let route = match afd_inference::Configured::new(inference, context) {
        Ok(route) if route.accepts_endpoint(model.base_url.as_deref()) => route,
        _ => return Ok(None),
    };
    let inference_host = route.into_host();
    Ok(Some(Registration {
        model,
        workspace_id: row.workspace_id,
        key_name: row.key_name,
        inference_host,
    }))
}

#[cfg(test)]
mod tests {
    /// Fixture fleets seeded through `afd_db` run on this reference, so the
    /// two spellings are one.
    #[test]
    fn the_fixture_setup_runs_on_the_platform_default() {
        assert_eq!(
            super::PLATFORM_MODEL_REF,
            afd_db::test_util::FIXTURE_SETUP_MODEL_REF
        );
    }
}
