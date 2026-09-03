//! This plane's half of the `OpenAPI` document.
//!
//! # Why the collector is per plane and not per daemon
//!
//! utoipa resolves `paths(...)` to a `__path_*` item generated beside each
//! handler, and those items cannot be named across a crate boundary when the
//! handler's module is private. This daemon's handlers are `pub(crate)` across
//! four plane crates, so a single collector at the composition root would mean
//! making 16 handlers public to serve a build-time tool. One collector per
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
    crate::handler::admin::libraries::delete,
    crate::handler::admin::libraries::list,
    crate::handler::admin::libraries::patch,
    crate::handler::admin::library_import::create,
    crate::handler::admin::models::create,
    crate::handler::admin::models::delete,
    crate::handler::admin::models::list,
    crate::handler::admin::models::update,
    crate::handler::admin::platform_keys::deactivate,
    crate::handler::admin::platform_keys::list,
    crate::handler::admin::platform_keys::set,
    crate::handler::operator::events::list,
    crate::handler::operator::leases::list,
    crate::handler::operator::runner_delete::handle,
    crate::handler::operator::runner_patch::handle,
    crate::handler::operator::runners::detail,
    crate::handler::operator::runners::list,
))]
struct Plane;

/// What the operator plane serves, as an `OpenAPI` document.
///
/// The platform's own surface and the operator's read over runners.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    Plane::openapi()
}
