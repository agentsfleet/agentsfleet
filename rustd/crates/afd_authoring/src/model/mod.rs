//! Model eligibility uses the workspace billing owner, never the chat creator's account.

use crate::{Result, error::ErrorKind};
use afd_core::id::Uuid7;
use afd_credential::provider::RegisteredModel;
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
