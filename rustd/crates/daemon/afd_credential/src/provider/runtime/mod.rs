//! Runtime and setup chat read the same owner-scoped persisted model metadata.

mod resolve;

use crate::error::Result;
use afd_core::id::Uuid7;
use afd_wire::inference::ModelInference;
use sqlx::PgConnection;

/// Non-secret runtime selection shown by human setup review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeModel {
    /// Workspace owner's registry reference or the explicit platform default.
    pub reference: String,
    /// Configured provider identifier.
    pub provider: String,
    /// Provider-native model identifier.
    pub model: String,
    /// Persisted model context window.
    pub context_cap_tokens: u32,
    /// Non-secret settings pinned with the selected model generation.
    pub inference: ModelInference,
    /// Credential, endpoint, catalogue and selection generation.
    pub resolution_version: String,
}

/// Resolves the exact configured and credentialed reference without fallback.
///
/// # Errors
/// Preserves datastore failures; unavailable references return `None`.
pub async fn runtime_model(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RuntimeModel>> {
    Ok(super::registered_model(connection, owner, reference)
        .await?
        .and_then(project))
}

fn project(model: super::RegisteredModel) -> Option<RuntimeModel> {
    let inference = model.inference?;
    Some(RuntimeModel {
        reference: model.reference.into_string(),
        provider: model.provider.into_string(),
        model: model.model.into_string(),
        context_cap_tokens: u32::try_from(model.context_window_tokens).ok()?,
        inference,
        resolution_version: model.resolution_version.into_string(),
    })
}
