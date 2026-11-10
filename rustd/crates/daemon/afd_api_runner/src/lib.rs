//! Runner-control HTTP adapters.
//!
//! Enrollment, heartbeat, lease, credential, memory, activity, and report
//! handlers live here so host protocol changes compile independently.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

pub use afd_http::{admission, auth, envelope, etag, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, get, patch, post, put};
use route::RunnerRoute;
use services::Services;

/// Selects the handler for a runner speaking for itself.
pub fn handler_for<D: Services>(verb: RunnerRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerRoute::SelfRecord => get(handler::runner::self_record::handle::<D>),
        RunnerRoute::Stream => post(handler::runner::stream::handle::<D>),
        RunnerRoute::ContextCheckpoint => context_checkpoint_handler::<D>(),
        RunnerRoute::TaskCheckpoints => post(handler::runner::task_checkpoints::plan::handle::<D>),
        RunnerRoute::TaskCheckpoint => {
            patch(handler::runner::task_checkpoints::commit::handle::<D>)
        }
        RunnerRoute::TaskCheckpointPage => {
            get(handler::runner::task_checkpoints::page_read::handle::<D>)
                .put(handler::runner::task_checkpoints::page_write::handle::<D>)
        }
        RunnerRoute::TaskCheckpointObjects => {
            get(handler::runner::task_checkpoints::objects_read::handle::<D>)
                .post(handler::runner::task_checkpoints::objects_register::handle::<D>)
        }
        RunnerRoute::TaskCheckpointUploads => {
            post(handler::runner::task_checkpoints::uploads::handle::<D>)
        }
        RunnerRoute::TaskCheckpointAuthorizations => {
            post(handler::runner::task_checkpoints::authorize::handle::<D>)
        }
        RunnerRoute::TaskCheckpointPin => {
            patch(handler::runner::task_checkpoints::pin::handle::<D>)
        }
        RunnerRoute::TaskInputReceipts => post(handler::runner::task_input_receipts::handle::<D>),
        RunnerRoute::TaskInputs => get(handler::runner::task_inputs::read::<D>),
        RunnerRoute::TaskBindings => post(handler::runner::task_bindings::accept::<D>),
        RunnerRoute::TaskEffects => get(handler::runner::task_effects::list::<D>)
            .post(handler::runner::task_effects::publish::<D>),
        RunnerRoute::TaskEffect => get(handler::runner::task_effects::read::<D>),
        RunnerRoute::TaskDecisions => post(handler::runner::decisions::reserve::<D>),
        RunnerRoute::TaskDecision => patch(handler::runner::decisions::record::<D>),
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

fn context_checkpoint_handler<D: Services>() -> MethodRouter<Arc<D>> {
    put(handler::runner::context::put::<D>)
}
