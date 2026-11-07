//! Parse protocol selection and held fleet identities before any placement.

use afd_core::id::Uuid7;
use afd_http::handler::Refusal;
use afd_wire::lease::{AcceptedKinds, LeaseRequest};
use garde::Validate as _;

pub(super) struct Request {
    pub(super) held: Vec<Uuid7>,
    pub(super) accepted: AcceptedKinds,
}

pub(super) fn read(body: &[u8]) -> Result<Request, Refusal> {
    let request: LeaseRequest<'_> = afd_http::handler::read_body(body).map_err(|_invalid| {
        Refusal::malformed("accepted_kinds and valid polling input are required")
    })?;
    request
        .validate()
        .map_err(|_invalid| Refusal::malformed("Held fleets exceed their bounds"))?;
    let held = request
        .holds
        .0
        .iter()
        .map(|id| {
            Uuid7::parse(id)
                .map_err(|_invalid| Refusal::malformed("Held fleets must be UUIDv7 identities"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Request {
        held,
        accepted: request.accepted_kinds,
    })
}
