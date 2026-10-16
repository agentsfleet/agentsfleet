//! Model eligibility uses the workspace billing owner, never the chat creator's account.

use crate::{Result, error::ErrorKind};
use afd_core::id::Uuid7;
use afd_credential::provider::RegisteredModel;
use afd_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};
use sqlx::PgConnection;

pub(crate) mod budget;
mod capability;
mod continuation;
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

pub(crate) use afd_inference::input_tokens;

pub(crate) async fn selected(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<RegisteredModel> {
    let selected = afd_credential::provider::registered_model(connection, owner, reference)
        .await?
        .ok_or(ErrorKind::Model)?;
    if !eligible(
        selected.inference.as_ref(),
        selected.base_url.as_deref(),
        selected.context_window_tokens,
        selected.input_nanos_per_million,
        selected.output_nanos_per_million,
    ) {
        return Err(ErrorKind::Model.into());
    }
    Ok(selected)
}

pub(crate) fn eligible(
    inference: Option<&afd_wire::inference::ModelInference>,
    endpoint: Option<&str>,
    context: i32,
    input: i64,
    output: i64,
) -> bool {
    let context = u32::try_from(context).unwrap_or_default();
    inference.is_some_and(|settings| {
        afd_inference::Configured::new(settings, context)
            .is_ok_and(|route| route.accepts_endpoint(endpoint))
    }) && input >= 0
        && output >= 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn configured_models_without_a_known_tokenizer_keep_a_conservative_bound() {
        let text = "語 and plain text";
        assert_eq!(
            super::input_tokens("workspace/model-next", text),
            text.len()
        );
        assert_eq!(super::input_tokens("gpt-4.1", "hello"), 1);
    }
}
