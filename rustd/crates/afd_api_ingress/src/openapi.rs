//! This plane's half of the `OpenAPI` document.
//!
//! # Why the collector is per plane and not per daemon
//!
//! utoipa resolves `paths(...)` to a `__path_*` item generated beside each
//! handler, and those items cannot be named across a crate boundary when the
//! handler's module is private. This daemon's handlers are `pub(crate)` across
//! four plane crates, so a single collector at the composition root would mean
//! making 8 handlers public to serve a build-time tool. One collector per
//! plane instead, merged by [`afd_api`], and nothing is published that the
//! router does not already serve.
//!
//! # Why the list is hand-kept and what stops it drifting
//!
//! It is a roster, and a roster is the thing somebody forgets. `mistral.rs`
//! keeps the same shape and its list has drifted; utoipa's own answer to that
//! is `OpenApiRouter`, which this daemon rejects because the total match in
//! `mount.rs` is the stronger invariant. What closes the gap here is the
//! coverage gate: a handler annotated and left out of this list is a served
//! route the document does not carry, and the gate names it.

use utoipa::OpenApi as _;

/// The collector. Private: [`document`] is the whole of what this module offers.
#[derive(utoipa::OpenApi)]
#[openapi(paths(
    crate::handler::events::receive,
    crate::handler::webhook::app_route::receive,
    crate::handler::webhook::approval_route::receive,
    crate::handler::webhook::github_route::receive,
    crate::handler::webhook::identity_route::receive,
    crate::handler::webhook::qstash_route::receive,
    crate::handler::webhook::receive_route::receive,
    crate::handler::webhook::svix_route::receive,
))]
struct Plane;

/// What the ingress plane serves, as an `OpenAPI` document.
///
/// Deliveries proven by a signature over the body rather than by a credential of ours.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    Plane::openapi()
}
