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

afd_http::plane_collector! {
    /// The collector. Private: [`document`] is the whole of what this module offers.
    struct Plane;
    paths(
        crate::handler::runner::activity::handle,
        crate::handler::runner::authoring::activity::handle,
        crate::handler::runner::authoring::model_call::handle,
        crate::handler::runner::authoring::renew::handle,
        crate::handler::runner::authoring::report::handle,
        crate::handler::runner::credential::handle,
        crate::handler::runner::task_checkpoints::plan::handle,
        crate::handler::runner::task_checkpoints::page_write::handle,
        crate::handler::runner::task_checkpoints::page_read::handle,
        crate::handler::runner::task_checkpoints::objects_register::handle,
        crate::handler::runner::task_checkpoints::objects_read::handle,
        crate::handler::runner::task_checkpoints::uploads::handle,
        crate::handler::runner::task_checkpoints::authorize::handle,
        crate::handler::runner::task_checkpoints::pin::handle,
        crate::handler::runner::task_checkpoints::commit::handle,
        crate::handler::runner::task_input_receipts::handle,
        crate::handler::runner::task_inputs::read,
        crate::handler::runner::task_bindings::accept,
        crate::handler::runner::task_effects::publish,
        crate::handler::runner::task_effects::read,
        crate::handler::runner::task_effects::list,
        crate::handler::runner::decisions::reserve,
        crate::handler::runner::decisions::record,
        crate::handler::runner::context::load,
        crate::handler::runner::context::put,
        crate::handler::runner::enrolment::handle,
        crate::handler::runner::heartbeat::handle,
        crate::handler::runner::lease::handle,
        crate::handler::runner::memory::capture,
        crate::handler::runner::tool_call::handle,
        crate::handler::runner::memory::hydrate,
        crate::handler::runner::memory::recall,
        crate::handler::runner::message::handle,
        crate::handler::runner::renew::handle,
        crate::handler::runner::report::handle,
        crate::handler::runner::schedule::create,
        crate::handler::runner::schedule::list,
        crate::handler::runner::schedule_edit::remove,
        crate::handler::runner::schedule_edit::update,
        crate::handler::runner::schedule_run::run,
        crate::handler::runner::schedule_run::runs,
        crate::handler::runner::self_record::handle,
    )
}

/// What the runner plane serves, as an `OpenAPI` document.
///
/// A runner speaking for itself: enrolment, liveness, the lease it runs, and what it reports back.
#[must_use]
pub fn document() -> utoipa::openapi::OpenApi {
    Plane::openapi()
}
