//! Strict typed bodies and one header key prevent alternate creation identities.

pub(super) use crate::handler::input::operation;
use crate::handler::{
    Refusal,
    input::{body, uuid},
};
use afd_api_wire::fleet as wire;
use afd_authoring::types::{SaveFleet, SaveSource, Start};

const INVALID: &str = "Fleet setup input violates the supported shape or bounds";
pub(super) fn save(bytes: &[u8]) -> Result<SaveFleet, Refusal> {
    let request: wire::SaveFleetRequest = body(bytes)?;
    let source = match request.source {
        wire::SaveSource::Draft { draft_candidate_id } => SaveSource::Draft {
            draft_candidate_id: uuid(&draft_candidate_id)?,
        },
        wire::SaveSource::Library {
            library_id,
            revision_id,
        } => SaveSource::Library {
            library_id,
            revision_id: uuid(&revision_id)?,
        },
    };
    let start = match request.start {
        wire::Start::None => Start::None,
        wire::Start::RunNow { message } => Start::RunNow { message },
    };
    Ok(SaveFleet {
        source,
        start,
        name: request.name,
        review_digest: request.review_digest,
    })
}

pub(super) fn patch(bytes: &[u8]) -> Result<wire::PatchFleetRequest, Refusal> {
    let request = garde::Unvalidated::new(body::<wire::PatchFleetRequest>(bytes)?)
        .validate()
        .map_err(|_invalid| Refusal::malformed(INVALID))?;
    Ok(request.into_inner())
}
