//! This plane's half of the `OpenAPI` document.
//!
//! # Why the collector is per plane and not per daemon
//!
//! utoipa resolves `paths(...)` to a `__path_*` item generated beside each
//! handler, and those items cannot be named across a crate boundary when the
//! handler's module is private. This daemon's handlers are `pub(crate)` across
//! four plane crates, so a single collector at the composition root would mean
//! making 10 handlers public to serve a build-time tool. One collector per
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
    crate::handler::runner::activity::handle,
    crate::handler::runner::bundle::handle,
    crate::handler::runner::credential::handle,
    crate::handler::runner::enrolment::handle,
    crate::handler::runner::heartbeat::handle,
    crate::handler::runner::lease::handle,
    crate::handler::runner::memory::capture,
    crate::handler::runner::memory::hydrate,
    crate::handler::runner::renew::handle,
    crate::handler::runner::report::handle,
    crate::handler::runner::self_record::handle,
))]
struct Plane;

/// What the runner plane serves, as an `OpenAPI` document.
///
/// A runner speaking for itself: enrolment, liveness, the lease it runs, and what it reports back.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    Plane::openapi()
}
