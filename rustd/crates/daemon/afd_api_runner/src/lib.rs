//! Runner-control HTTP adapters.
//!
//! The stream, self-record, credential, memory, message, schedule, fleet steer,
//! fleets, tool-call, decision and task handlers live here so host protocol
//! changes compile independently. Heartbeats, runs, renewals, activity and reports
//! travel on the stream.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

pub use afd_http::{auth, envelope, request_id, route, services};

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use axum::routing::{MethodRouter, get, patch, post};
use handler::runner::task_checkpoints as checkpoints;
use route::RunnerRoute;
use services::Services;

/// Selects the handler for a runner speaking for itself.
pub fn handler_for<D: Services>(verb: RunnerRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerRoute::SelfRecord => get(handler::runner::self_record::handle::<D>),
        RunnerRoute::Stream => post(handler::runner::stream::handle::<D>),
        RunnerRoute::TaskCheckpoints => post(checkpoints::plan::handle::<D>),
        RunnerRoute::TaskCheckpoint => patch(checkpoints::commit::handle::<D>),
        RunnerRoute::TaskCheckpointPage => {
            get(checkpoints::page_read::handle::<D>).put(checkpoints::page_write::handle::<D>)
        }
        RunnerRoute::TaskCheckpointObjects => get(checkpoints::objects_read::handle::<D>)
            .post(checkpoints::objects_register::handle::<D>),
        RunnerRoute::TaskCheckpointUploads => post(checkpoints::uploads::handle::<D>),
        RunnerRoute::TaskCheckpointAuthorizations => post(checkpoints::authorize::handle::<D>),
        RunnerRoute::TaskCheckpointPin => patch(checkpoints::pin::handle::<D>),
        RunnerRoute::TaskInputReceipts => post(handler::runner::task_input_receipts::handle::<D>),
        RunnerRoute::TaskInputs => get(handler::runner::task_inputs::read::<D>),
        RunnerRoute::TaskBindings => post(handler::runner::task_bindings::accept::<D>),
        RunnerRoute::TaskEffects => get(handler::runner::task_effects::list::<D>)
            .post(handler::runner::task_effects::publish::<D>),
        RunnerRoute::TaskEffect => get(handler::runner::task_effects::read::<D>),
        RunnerRoute::TaskDecisions => post(handler::runner::decisions::reserve::<D>),
        RunnerRoute::TaskDecision => patch(handler::runner::decisions::record::<D>),
        RunnerRoute::MemoryLoad => get(handler::runner::memory::load::<D>),
        RunnerRoute::MemoryCapture => post(handler::runner::memory::capture::<D>),
        RunnerRoute::MemoryRecall => post(handler::runner::memory::recall::<D>),
        RunnerRoute::Items => post(handler::runner::tool_call::handle::<D>),
        RunnerRoute::CredentialsMint => post(handler::runner::credential::handle::<D>),
        RunnerRoute::Schedules => {
            get(handler::runner::schedule::list::<D>).post(handler::runner::schedule::create::<D>)
        }
        RunnerRoute::Schedule => patch(handler::runner::schedule_edit::update::<D>)
            .delete(handler::runner::schedule_edit::remove::<D>),
        RunnerRoute::ScheduleRuns => get(handler::runner::schedule_run::runs::<D>)
            .post(handler::runner::schedule_run::run::<D>),
        RunnerRoute::Messages => post(handler::runner::message::handle::<D>),
        RunnerRoute::Steers => post(handler::runner::fleet_steer::handle::<D>),
        RunnerRoute::Fleets => get(handler::runner::fleets::list::<D>),
        RunnerRoute::FleetTurns => get(handler::runner::fleets::events::<D>),
    }
}
