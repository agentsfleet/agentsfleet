//! `POST /v1/runners/me/leases` — the poll a runner lives on.
//!
//! Thinner than it looks, and deliberately: the whole decision — claim, money,
//! gates, policy, row — is `afd_fleet::lease::Plane`'s, and it answers with
//! the bytes it decided on. What is left here is the two things that are
//! genuinely this layer's: which identity is asking, and what a failure looks
//! like on the wire.
//!
//! # The body names the runner's holds, and nothing else
//!
//! A runner lists the fleets whose sandboxes it holds frozen, so a held
//! fleet's next event reaches it on its next poll whatever partition the
//! fleet's readiness mark sits in (`afd_fleet::lease::assign`). That is all
//! the body says: one shape is served unconditionally, with no negotiation,
//! no downgrade and no "unsupported version" refusal. The body is read
//! LENIENTLY, as the heartbeat's is: an empty, unreadable or out-of-bounds
//! body reads as holding nothing, so a poll never fails over what it carries,
//! and the list only reorders what the candidate scan would offer anyway.
//!
//! # Always 200, and never 204
//!
//! Work and no-work are the same status and the same shape —
//! `{"lease":…,"retry_after_ms":…}`. A 204 would make "nothing to do" a
//! different response class from "here is something to do", and a runner would
//! need two parsers for one poll.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_runner::heartbeat::holds::{fleets, prove};
use afd_wire::lease::LeaseRequest;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed poll is logged under.
const EVENT: &str = "runner_lease_failed";

/// The content type the answer is already serialized as.
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");

/// Hands the runner its next lease, or a backoff.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::RUNNER_LEASES,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_lease",
    summary = "Poll for the next lease",
    description = concat!(
        "The poll a runner lives on. Answers either a lease to run or a ",
        "backoff to wait out. The claim, the money, the gates and the policy ",
        "are all decided before the answer is written. The body lists the ",
        "fleets whose sandboxes the runner holds, which are tried first. An ",
        "empty or unreadable body holds nothing. ",
    ),
    request_body = Option<LeaseRequest>,
    responses(
        (status = 200, description = "A lease to run, or a backoff to wait out", body = afd_wire::lease::LeaseResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Response {
    let held = holds(&body);
    match services
        .leases()
        .lease(runner.id(), &held, runner.is_degraded(), services.now())
        .await
    {
        // Already JSON, because the policy inside it borrows from values that
        // do not outlive the call that built them — see `lease::pull`. Handing
        // back bytes rather than a `Json<T>` is what keeps a single assembly.
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, APPLICATION_JSON)],
            body,
        )
            .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The fleets the poll says the runner holds, or none: an empty, unreadable or
/// out-of-bounds body holds nothing, and the poll is answered all the same. An
/// entry inside the bounds that still is not an identifier is dropped alone.
/// The list is proved by the rule the heartbeat's is.
fn holds(body: &[u8]) -> Vec<Uuid7> {
    if body.is_empty() {
        return Vec::new();
    }
    afd_http::handler::read_body::<LeaseRequest<'_>>(body)
        .ok()
        .and_then(|request| prove(request.holds))
        .map(|proved| fleets(&proved))
        .unwrap_or_default()
}

/// What a poll's body reads as. `holds` returns a list and never an error, so
/// no body can fail the poll; these pin WHICH list each body reads as.
#[cfg(test)]
mod tests {
    use afd_wire::runner::{FLEET_ID_TEXT_BYTES, HOLDS_MAX};

    use super::holds;

    /// Two fleets in the canonical version-7 form a runner sends.
    const FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a8057";
    const OTHER_FLEET: &str = "01890a5d-ac96-774b-8cce-b302099a8058";

    /// A poll that holds nothing.
    const NOTHING: [String; 0] = [];

    /// A body listing `fleets` as held.
    fn body(fleets: &[&str]) -> String {
        let quoted: Vec<String> = fleets.iter().map(|fleet| format!("\"{fleet}\"")).collect();
        format!("{{\"holds\":[{}]}}", quoted.join(","))
    }

    /// What `body` reads as, in text, so a failure prints the identifiers.
    fn read(body: &str) -> Vec<String> {
        holds(body.as_bytes())
            .iter()
            .map(|fleet| fleet.as_str().to_owned())
            .collect()
    }

    #[test]
    fn test_an_empty_body_or_an_absent_list_holds_nothing() {
        assert_eq!(read(""), NOTHING, "the body a runner predating holds sends");
        assert_eq!(read("{}"), NOTHING, "an absent list decodes as empty");
    }

    #[test]
    fn test_an_unreadable_body_holds_nothing() {
        for unreadable in ["not-json", "{\"holds\":", "[]", "{\"held\":[]}"] {
            assert_eq!(read(unreadable), NOTHING, "{unreadable}");
        }
    }

    #[test]
    fn test_a_list_past_the_bound_holds_nothing_rather_than_its_first_entries() {
        let at_bound = vec![FLEET; HOLDS_MAX];
        assert_eq!(
            read(&body(&at_bound)).len(),
            HOLDS_MAX,
            "the bound itself is read"
        );

        let past_bound = vec![FLEET; HOLDS_MAX + 1];
        assert_eq!(read(&body(&past_bound)), NOTHING);
        assert_eq!(
            read(&body(&[FLEET, "not-a-fleet"])),
            NOTHING,
            "an entry of the wrong length is out of bounds too, and takes the list with it"
        );
    }

    #[test]
    fn test_an_entry_that_is_not_an_identifier_is_dropped_and_the_rest_kept() {
        let not_an_id = "x".repeat(FLEET_ID_TEXT_BYTES);
        assert_eq!(
            read(&body(&[FLEET, &not_an_id, OTHER_FLEET])),
            [FLEET, OTHER_FLEET]
        );
    }

    #[test]
    fn test_a_valid_list_holds_every_fleet_in_order() {
        assert_eq!(read(&body(&[OTHER_FLEET, FLEET])), [OTHER_FLEET, FLEET]);
    }
}
