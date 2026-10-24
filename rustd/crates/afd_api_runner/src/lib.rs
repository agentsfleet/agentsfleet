//! Runner-control HTTP adapters.
//!
//! Enrollment, heartbeat, lease, credential, memory, activity, and report
//! handlers live here so host protocol changes compile independently.

#![deny(clippy::self_named_module_files)]

pub use afd_http::{admission, auth, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, get, patch, post};
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
        RunnerRoute::AuthoringActivity => post(handler::runner::authoring::activity::handle::<D>),
        RunnerRoute::AuthoringModelCalls => {
            post(handler::runner::authoring::model_call::handle::<D>)
        }
        RunnerRoute::AuthoringRenew => post(handler::runner::authoring::renew::handle::<D>),
        RunnerRoute::AuthoringReports => post(handler::runner::authoring::report::handle::<D>),
        RunnerRoute::ContextCheckpoint => {
            get(handler::runner::context::load::<D>).put(handler::runner::context::put::<D>)
        }
        RunnerRoute::MemoryHydrate => get(handler::runner::memory::hydrate::<D>),
        RunnerRoute::MemoryCapture => post(handler::runner::memory::capture::<D>),
        RunnerRoute::MemoryRecall => post(handler::runner::memory::recall::<D>),
        RunnerRoute::ToolCalls => post(handler::runner::tool_call::handle::<D>),
        RunnerRoute::CredentialsMint => post(handler::runner::credential::handle::<D>),
        RunnerRoute::Schedules => {
            get(handler::runner::schedule::list::<D>).post(handler::runner::schedule::create::<D>)
        }
        RunnerRoute::Schedule => patch(handler::runner::schedule_edit::update::<D>)
            .delete(handler::runner::schedule_edit::remove::<D>),
        RunnerRoute::ScheduleRuns => get(handler::runner::schedule_run::runs::<D>)
            .post(handler::runner::schedule_run::run::<D>),
        RunnerRoute::Messages => post(handler::runner::message::handle::<D>),
    }
}

/// Returns the unauthenticated runner-enrolment handler.
pub fn enrolment_handler<D: Services>() -> MethodRouter<Arc<D>> {
    post(handler::runner::enrolment::handle::<D>)
}
