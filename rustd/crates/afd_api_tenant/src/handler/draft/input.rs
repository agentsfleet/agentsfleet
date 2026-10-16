//! Parse strict objects without logging private text or rejected values.

use crate::handler::Refusal;
use crate::handler::input::uuid;
pub(super) use crate::handler::input::{body, expected};
use afd_api_wire::authoring as wire;
use afd_authoring::types::{CreateDraft, DraftPatch, SendMessage, Source};
use afd_fleet_runtime::{
    Definition,
    definition::{ConfigOp, InstructionEdit},
};
use http::HeaderMap;

const INVALID: &str = "Setup input violates the supported shape or bounds";
const HEADER_KEY: &str = "Use only body operation_id for draft creation and messages";
const IDEMPOTENCY_KEY: &str = "idempotency-key";

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
        wire::Source::Library {
            library_id,
            revision_id,
        } => Source::Library {
            library_id,
            revision_id: uuid(&revision_id)?,
        },
        wire::Source::Fleet {
            fleet_id,
            setup_revision,
        } => Source::Fleet {
            fleet_id: uuid(&fleet_id)?,
            setup_revision,
        },
    };
    Ok(CreateDraft {
        source,
        message: request.message,
        authoring_model_ref: request.authoring_model_ref,
        operation_id: uuid(&request.operation_id)?,
    })
}

pub(super) fn patch(bytes: &[u8]) -> Result<DraftPatch, Refusal> {
    let request: wire::DraftPatchRequest<ConfigOp, InstructionEdit> = body(bytes)?;
    Ok(DraftPatch {
        title: request.title,
        instance_name: request.instance_name,
        authoring_model_ref: request.authoring_model_ref,
        runtime_model_ref: request.runtime_model_ref,
        config_ops: request.config_ops,
        instruction_edits: request.instruction_edits,
    })
}

pub(super) fn send(bytes: &[u8]) -> Result<SendMessage, Refusal> {
    let request: wire::MessageRequest = body(bytes)?;
    Ok(SendMessage {
        message: request.message,
        operation_id: uuid(&request.operation_id)?,
    })
}

pub(super) fn cutoff(bytes: &[u8]) -> Result<i64, Refusal> {
    let request: wire::CancellationRequest = body(bytes)?;
    let request = garde::Unvalidated::new(request)
        .validate()
        .map_err(|_invalid| Refusal::malformed(INVALID))?;
    request
        .through_sequence
        .parse()
        .map_err(|_invalid| Refusal::malformed(INVALID))
}
