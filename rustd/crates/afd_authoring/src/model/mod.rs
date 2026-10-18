//! Model eligibility uses the workspace billing owner, never the chat creator's account.

use crate::{Result, error::ErrorKind};
use afd_core::id::Uuid7;
use afd_credential::provider::{PLATFORM_MODEL_REF, RegisteredModel};
use afd_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};
use sqlx::PgConnection;

pub(crate) mod budget;
mod capability;
mod dispatch;
mod prepare;
mod provider;
mod receipt;
mod sql;

pub use dispatch::ModelCalls;
pub use provider::{Network, ProviderRequest, ProviderResponse, Transport};

/// Model proposals use the same native types as drawer and activity edits.
pub type ModelCallInput = afd_wire::authoring::ModelCall<Definition, ConfigOp, InstructionEdit>;
/// A bounded result containing validated native proposal shapes.
pub type ModelCallResult = afd_wire::authoring::ModelResult<Definition, ConfigOp, InstructionEdit>;

const OPENAI: &str = "openai";

/// The `authoring.dispatches.status` words, and the knowledge word a receipt's
/// usage carries when its provider figures are authoritative.
pub(crate) mod dispatch_status {
    /// Intent recorded; no provider answer yet.
    pub(crate) const PENDING: &str = "pending";
    /// The physical request's outcome cannot be known.
    pub(crate) const UNKNOWN: &str = "unknown";
    /// A decoded provider result.
    pub(crate) const COMPLETED: &str = "completed";
    /// The provider answered with a failure and known usage.
    pub(crate) const FAILED: &str = "failed";
    /// `usage->>'knowledge'` of a receipt with authoritative figures.
    pub(crate) const KNOWN: &str = "known";
}

/// The `tenant_model_id` a model reference is stored as: NULL names the
/// platform default, any other reference is the tenant model's own id.
pub(crate) fn tenant_model(reference: &str) -> Option<&str> {
    (reference != PLATFORM_MODEL_REF).then_some(reference)
}

/// The model reference a stored `tenant_model_id` spells.
pub(crate) fn reference(tenant_model: Option<String>) -> String {
    tenant_model.unwrap_or_else(|| PLATFORM_MODEL_REF.to_owned())
}

pub(crate) async fn selected(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<RegisteredModel> {
    let selected = afd_credential::provider::registered_model(connection, owner, reference)
        .await?
        .ok_or(ErrorKind::Model)?;
    // Only transports with a local, recognized tokenizer and authoritative
    // usage can be admitted. An unknown model never guesses an encoding.
    if !eligible(
        &selected.provider,
        &selected.model,
        selected.context_window_tokens,
        selected.input_nanos_per_million,
        selected.output_nanos_per_million,
    ) {
        return Err(ErrorKind::Model.into());
    }
    Ok(selected)
}

pub(crate) fn eligible(provider: &str, model: &str, context: i32, input: i64, output: i64) -> bool {
    provider == OPENAI
        && context > 0
        && input >= 0
        && output >= 0
        && tiktoken_rs::tokenizer::get_tokenizer(model).is_some()
        && text_transport(model)
}

fn text_transport(model: &str) -> bool {
    // This adapter supports stateless text/function Responses calls. Audio,
    // realtime and reasoning transports require different continuation rules.
    ["gpt-4o", "gpt-4.1"]
        .iter()
        .any(|family| model.starts_with(family))
        && !["audio", "realtime", "transcribe", "tts", "search"]
            .iter()
            .any(|kind| model.contains(kind))
}
