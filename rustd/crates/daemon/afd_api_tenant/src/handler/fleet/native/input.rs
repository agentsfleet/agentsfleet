//! A fleet's create and change bodies, bounded before they reach the store.

use af_validate::{Sentences, TEXT_TOO_LONG, nul_free};
use afd_api_wire::fleet::{CreateFleetRequest, PatchFleetRequest};
use garde::{Unvalidated, Valid};

use crate::handler::{Refusal, input::body};

/// The refusal an empty first message earns.
const DETAIL_MESSAGE_EMPTY: &str = "message must not be empty";

/// The refusal a first message holding NUL earns.
const DETAIL_MESSAGE_NUL: &str = "message must not contain a NUL character";

/// The refusal an unusable retry key earns.
const DETAIL_OPERATION_ID: &str = "operation_id must be a UUIDv7";

/// The refusal a model reference outside its bound earns.
const DETAIL_MODEL_REF: &str = "model_ref must be 1 to 1,048,576 characters with no NUL character";

/// The refusal a name outside a fleet name's bound earns.
const DETAIL_NAME: &str = "name must be 1 to 64 lower-case letters, digits or hyphens";

/// A refused create's sentence by the field its report names; the message,
/// which every other refusal reports, answers the text ceiling's own.
const CREATE_SENTENCES: Sentences = Sentences::new(
    &[
        ("operation_id", DETAIL_OPERATION_ID),
        ("model_ref", DETAIL_MODEL_REF),
        ("name", DETAIL_NAME),
    ],
    TEXT_TOO_LONG,
);

/// A refused change's sentence by the field its report names.
const PATCH_SENTENCES: Sentences = Sentences::new(&[("model_ref", DETAIL_MODEL_REF)], DETAIL_NAME);

/// The create body, proved inside its bounds.
///
/// # Errors
/// A body that is not the create's shape, and the sentence of the first field
/// out of its bound; a message past the ceiling answers the ceiling's own.
pub(super) fn create(bytes: &[u8]) -> Result<Valid<CreateFleetRequest>, Refusal> {
    // The route bounds the full body by the runner frame. The ordinary fleet
    // patch parser keeps its smaller text-only envelope.
    let request: CreateFleetRequest = af_core::json::object_from_slice(bytes)
        .map_err(|_malformed| Refusal::malformed("Input violates the supported shape or bounds"))?;
    let message_empty = request.message.is_empty();
    let message_nul = nul_free(&request.message, &()).is_err();
    Unvalidated::new(request).validate().map_err(|report| {
        let picked = CREATE_SENTENCES.pick(&report);
        if picked != TEXT_TOO_LONG {
            Refusal::malformed(picked)
        } else if message_empty {
            Refusal::malformed(DETAIL_MESSAGE_EMPTY)
        } else if message_nul {
            Refusal::malformed(DETAIL_MESSAGE_NUL)
        } else {
            Refusal::input_too_long()
        }
    })
}

/// The change body, proved inside its bounds.
///
/// # Errors
/// A body that is not the change's shape, and the sentence of the first field
/// out of its bound.
pub(super) fn patch(bytes: &[u8]) -> Result<Valid<PatchFleetRequest>, Refusal> {
    Unvalidated::new(body::<PatchFleetRequest>(bytes)?)
        .validate()
        .map_err(|report| Refusal::malformed(PATCH_SENTENCES.pick(&report)))
}
