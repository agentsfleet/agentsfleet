//! Every refusal this daemon writes, as one product event.
//!
//! # Why a layer and not a call in each handler
//!
//! A refusal is written in a dozen places — a handler's `?`, the scope rung,
//! the ownership layer, the admission shed, the guard that could not read a
//! credential — and only some of them are handlers at all. Reporting from each
//! would be a dozen call sites to add and a dozen to forget, and the ones that
//! matter most under an incident (auth, ceiling) are exactly the ones that
//! never reach a handler. One layer over the response sees all of them.
//!
//! # The event a refusal becomes is decided by its CODE
//!
//! `UZ-AUTH-*` is a credential this daemon would not take, and the funnel that
//! asks "how many people could not sign in" reads `auth_rejected`. A spend or
//! policy refusal is an entitlement boundary, and the funnel that asks "who is
//! hitting their plan" reads `entitlement_rejected`. Everything else is
//! `api_error`. Reading the code rather than the status is what keeps a 403
//! from the scope rung apart from a 403 the gate raised.
//!
//! # This is new emission, and it is deliberate
//!
//! The daemon this ports DECLARES all three of these events and captures none
//! of them — `telemetry_events.zig` has the structs, and the only references
//! outside it are in `telemetry_test.zig`. The event names, property keys and
//! shapes are still the Zig's, so nothing downstream has to learn a new one;
//! what changes is that they now fire.

use std::sync::Arc;

use afd_auth::principal::Principal;
use afd_core::error_code::ErrorCode;
use afd_observability::Telemetry;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::auth::Owned;
use crate::envelope::Refused;
use crate::services::Services;

/// The distinct id an unattributable refusal is reported under.
///
/// `distinctIdOrSystem`, mirrored. A refusal written before a credential was
/// read has nobody behind it, and `PostHog` requires a distinct id — so they all
/// collapse to one non-person rather than each inventing an identity that would
/// make the funnel count anonymous refusals as unique people.
const ACTOR_SYSTEM: &str = "system";

/// The family a credential refusal carries.
const FAMILY_AUTH: &str = "AUTH";

/// The families an entitlement boundary refuses under.
///
/// The gate's own codes: a run refused for money, for policy, or for an
/// approval it does not hold. Grouped because the funnel asks one question of
/// all three — "who is hitting a limit" — and splitting them would make that
/// question three charts nobody adds up.
const ENTITLEMENT_FAMILIES: &[&str] = &["BILLING", "GATE", "REPAIR"];

/// Reports the refusal a response carries, if it carries one.
///
/// Runs after the inner service, so it sees what every layer beneath it wrote.
/// A response with no [`Refused`] extension is a success and reports nothing.
pub(crate) async fn record<D: Services>(
    State(services): State<Arc<D>>,
    request: Request,
    next: Next,
) -> Response {
    // Read BEFORE the request is consumed: the extensions travel with the
    // request, and the layers that put them there run beneath this one, so
    // this reads what the guard and the ownership layer left on the way down.
    let response = next.run(request).await;
    let Some(refused) = response.extensions().get::<Refused>().cloned() else {
        return response;
    };
    let actor = actor_of(&response);
    let workspace = response
        .extensions()
        .get::<Owned>()
        .map(|owned| owned.workspace.as_str().to_owned());
    services
        .analytics()
        .report(&telemetry_of(&refused, actor, workspace));
    response
}

/// Who the refusal happened to, when the guard got far enough to say.
fn actor_of(response: &Response) -> String {
    response
        .extensions()
        .get::<Principal>()
        .and_then(Principal::person)
        .map_or_else(
            || ACTOR_SYSTEM.to_owned(),
            |person| person.subject().as_str().to_owned(),
        )
}

/// The event this refusal is, decided by its registry code.
fn telemetry_of(refused: &Refused, actor: String, workspace: Option<String>) -> Telemetry {
    match family_of(refused.code) {
        FAMILY_AUTH => Telemetry::AuthRejected {
            // The CODE, never the detail: a detail can name a credential shape
            // or a subject, and this record is read by everyone with dashboard
            // access. The code says which wall refused and nothing else.
            reason: refused.code.as_str().to_owned(),
            request_id: refused.request_id.clone(),
        },
        family if ENTITLEMENT_FAMILIES.contains(&family) => Telemetry::EntitlementRejected {
            actor,
            workspace_id: workspace.unwrap_or_default(),
            boundary: family.to_owned(),
            reason_code: refused.code.as_str().to_owned(),
            request_id: refused.request_id.clone(),
        },
        _other => Telemetry::ApiError {
            actor,
            error_code: refused.code.as_str().to_owned(),
            message: refused.detail.clone(),
            workspace_id: workspace,
            request_id: refused.request_id.clone(),
        },
    }
}

/// The middle segment of `UZ-FAMILY-NNN`.
///
/// Every code in the registry has exactly three segments — `error_code.rs`'s
/// own test proves it — so the empty string here is unreachable for a declared
/// code and lands in the `api_error` arm, which is the right home for a code
/// this mapping does not recognise.
fn family_of(code: ErrorCode) -> &'static str {
    code.as_str().split('-').nth(1).unwrap_or_default()
}

#[cfg(test)]
mod tests;
