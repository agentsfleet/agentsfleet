//! Runner-control HTTP adapters.
//!
//! Enrollment, heartbeat, lease, credential, memory, activity, and report
//! handlers live here so host protocol changes compile independently.

pub use afd_http::{admission, auth, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, get, post};
use route::RunnerRoute;
use services::Services;

/// Selects the handler for a runner speaking for itself.
pub fn handler_for<D: Services>(verb: RunnerRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerRoute::SelfRecord => get(handler::runner::self_record::handle::<D>),
        RunnerRoute::Heartbeat => post(handler::runner::heartbeat::handle::<D>),
        RunnerRoute::Lease => post(handler::runner::lease::handle::<D>),
        RunnerRoute::Report => post(handler::runner::report::handle::<D>),
        RunnerRoute::Renew => post(handler::runner::renew::handle::<D>),
        RunnerRoute::Activity => post(handler::runner::activity::handle::<D>),
        RunnerRoute::MemoryHydrate => get(handler::runner::memory::hydrate::<D>),
        RunnerRoute::MemoryCapture => post(handler::runner::memory::capture::<D>),
        RunnerRoute::Bundle => get(handler::runner::bundle::handle::<D>),
        RunnerRoute::CredentialsMint => post(handler::runner::credential::handle::<D>),
    }
}

/// Returns the unauthenticated runner-enrolment handler.
pub fn enrolment_handler<D: Services>() -> MethodRouter<Arc<D>> {
    post(handler::runner::enrolment::handle::<D>)
}
