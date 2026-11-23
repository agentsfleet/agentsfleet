//! Parse strict objects without logging private text or rejected values.

use crate::handler::Refusal;
use crate::handler::input::uuid;
pub(super) use crate::handler::input::{body, expected};
use af_fleet_runtime::{
    Definition,
    definition::{ConfigOp, InstructionEdit},
};
use af_validate::{Sentences, TEXT_TOO_LONG};
use afd_api_wire::setup as wire;
use afd_setup::types::{CreateDraft, DraftPatch, SendMessage, Source};
use garde::Validate as _;
use http::HeaderMap;

const HEADER_KEY: &str = "Use only body operation_id for draft creation and messages";
const IDEMPOTENCY_KEY: &str = "idempotency-key";

/// The refusal setup input outside its shape or bounds earns, as the setup
/// domain answers it (`afd_setup::Error::detail`).
const INVALID: &str = "Setup input violates the supported shape or bounds";

/// A refused message's sentence by the field its report names: the message
/// past the text ceiling is told the ceiling, a number a person can act on.
const SENTENCES: Sentences = Sentences::new(&[("message", TEXT_TOO_LONG)], INVALID);

pub(super) fn body_key(headers: &HeaderMap) -> Result<(), Refusal> {
    if headers.contains_key(IDEMPOTENCY_KEY) {
        return Err(Refusal::malformed(HEADER_KEY));
    }
    Ok(())
}

pub(super) fn create(bytes: &[u8]) -> Result<CreateDraft, Refusal> {
    let request: wire::CreateDraftRequest<Definition> = body(bytes)?;
    let source = match request.source {
        wire::Source::Blank => Source::Blank,
        wire::Source::Definition { definition } => Source::Definition {
            definition: Box::new(definition),
        },
        wire::Source::Fleet {
            fleet_id,
            setup_revision,
        } => Source::Fleet {
            fleet_id: uuid(&fleet_id)?,
            setup_revision,
        },
    };
    let create = CreateDraft {
        source,
        message: request.message,
        save: request.save,
        setup_model_ref: request.setup_model_ref,
        operation_id: uuid(&request.operation_id)?,
    };
    create
        .validate()
        .map_err(|report| refused(&report, create.message.as_deref()))?;
    Ok(create)
}

pub(super) fn patch(bytes: &[u8]) -> Result<DraftPatch, Refusal> {
    let request: wire::DraftPatchRequest<ConfigOp, InstructionEdit> = body(bytes)?;
    Ok(DraftPatch {
        title: request.title,
        instance_name: request.instance_name,
        setup_model_ref: request.setup_model_ref,
        runtime_model_ref: request.runtime_model_ref,
        config_ops: request.config_ops,
        instruction_edits: request.instruction_edits,
    })
}

pub(super) fn send(bytes: &[u8]) -> Result<SendMessage, Refusal> {
    let request: wire::MessageRequest = body(bytes)?;
    let send = SendMessage {
        message: request.message,
        save: request.save,
        operation_id: uuid(&request.operation_id)?,
    };
    send.validate()
        .map_err(|report| refused(&report, Some(send.message.as_str())))?;
    Ok(send)
}

/// The refusal a request garde refused earns, decided before the setup
/// domain is called, so a refused message is never stored.
///
/// A message's ceiling, its emptiness and its NUL rule all report at
/// `message`, so the text tells them apart: an empty message, or one holding
/// NUL, earns the shape sentence, and any other the ceiling's own refusal,
/// under the code whose dashboard sentence names the ceiling.
fn refused(report: &garde::Report, message: Option<&str>) -> Refusal {
    let shape =
        message.is_some_and(|text| text.is_empty() || af_validate::nul_free(text, &()).is_err());
    match (shape, SENTENCES.pick(report)) {
        (true, _) => Refusal::malformed(INVALID),
        (false, TEXT_TOO_LONG) => Refusal::input_too_long(),
        (false, sentence) => Refusal::malformed(sentence),
    }
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
