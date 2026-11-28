//! Runner-control HTTP adapters.
//!
//! The stream, self-record, credential, memory, message, schedule, trigger,
//! access, fleet steer, fleets, tool-call, decision and task handlers live
//! here so host protocol
//! changes compile independently. Heartbeats, runs, renewals, activity and reports
//! travel on the stream.

#![deny(unused_crate_dependencies)]
#![deny(clippy::self_named_module_files)]

use afd_http::{auth, route};

pub mod services;

pub(crate) mod handler;

// The document generator, compiled only when it is asked for.
#[cfg(feature = "openapi")]
pub mod openapi;

use std::sync::Arc;

use af_wire::protocol::MESSAGE_BYTES_MAX;
use afd_http::handler::setup_input::TEXT_BODY_BYTES_MAX;
use axum::extract::DefaultBodyLimit;
use axum::routing::{MethodRouter, delete, get, patch, post};
use handler::runner::durable_task_checkpoints as checkpoints;
use route::RunnerRoute;
use services::RunnerPlane;

/// Selects the handler for a runner speaking for itself.
pub fn handler_for<D: RunnerPlane>(verb: RunnerRoute) -> MethodRouter<Arc<D>> {
    match verb {
        RunnerRoute::SelfRecord => get(handler::runner::self_record::handle::<D>),
        RunnerRoute::Stream => post(handler::runner::stream::handle::<D>),
        RunnerRoute::DurableTaskCheckpoints => post(checkpoints::plan::handle::<D>),
        RunnerRoute::DurableTaskCheckpoint => patch(checkpoints::commit::handle::<D>),
        RunnerRoute::DurableTaskCheckpointPage => {
            get(checkpoints::page_read::handle::<D>).put(checkpoints::page_write::handle::<D>)
        }
        RunnerRoute::DurableTaskCheckpointObjects => get(checkpoints::objects_read::handle::<D>)
            .post(checkpoints::objects_register::handle::<D>),
        RunnerRoute::DurableTaskCheckpointUploads => post(checkpoints::uploads::handle::<D>),
        RunnerRoute::DurableTaskCheckpointAuthorizations => {
            post(checkpoints::authorize::handle::<D>)
        }
        RunnerRoute::DurableTaskCheckpointPin => patch(checkpoints::pin::handle::<D>),
        RunnerRoute::DurableTaskInputReceipts => {
            post(handler::runner::durable_task_input_receipts::handle::<D>)
        }
        RunnerRoute::DurableTaskInputs => get(handler::runner::durable_task_inputs::read::<D>),
        RunnerRoute::DurableTaskBindings => {
            post(handler::runner::durable_task_bindings::accept::<D>)
        }
        RunnerRoute::DurableTaskEffects => get(handler::runner::durable_task_effects::list::<D>)
            .post(handler::runner::durable_task_effects::publish::<D>),
        RunnerRoute::DurableTaskEffect => get(handler::runner::durable_task_effects::read::<D>),
        RunnerRoute::DurableTaskDecisions => post(handler::runner::decisions::reserve::<D>),
        RunnerRoute::DurableTaskDecision => patch(handler::runner::decisions::record::<D>),
        RunnerRoute::MemoryLoad => get(handler::runner::memory::load::<D>),
        // A push carries every entry a run stored, each at the text ceiling,
        // so it is bounded by the runner frame alone.
        RunnerRoute::MemoryCapture => post(handler::runner::memory::capture::<D>)
            .layer(DefaultBodyLimit::max(MESSAGE_BYTES_MAX)),
        RunnerRoute::MemoryRecall => post(handler::runner::memory::recall::<D>),
        // A post of full tool records is bounded by the runner frame alone,
        // past axum's 2 MiB default: no field, event or post cap applies.
        RunnerRoute::Items => post(handler::runner::tool_call::handle::<D>)
            .layer(DefaultBodyLimit::max(MESSAGE_BYTES_MAX)),
        RunnerRoute::CredentialsMint => post(handler::runner::credential::handle::<D>),
        RunnerRoute::Schedules => {
            get(handler::runner::schedule::list::<D>).post(handler::runner::schedule::create::<D>)
        }
        RunnerRoute::Schedule => patch(handler::runner::schedule_edit::update::<D>)
            .delete(handler::runner::schedule_edit::remove::<D>),
        RunnerRoute::ScheduleRuns => get(handler::runner::schedule_run::runs::<D>)
            .post(handler::runner::schedule_run::run::<D>),
        RunnerRoute::Triggers => {
            get(handler::runner::trigger::list::<D>).post(handler::runner::trigger::add::<D>)
        }
        RunnerRoute::Trigger => delete(handler::runner::trigger::remove::<D>),
        RunnerRoute::Searches => post(handler::runner::web::search::<D>),
        RunnerRoute::Extracts => post(handler::runner::web::extract::<D>),
        RunnerRoute::Access => post(handler::runner::access::request::<D>),
        RunnerRoute::Messages => post(handler::runner::message::handle::<D>),
        // A fleet's message to another carries text up to the one ceiling,
        // past axum's 2 MiB default once escaped.
        RunnerRoute::Steers => post(handler::runner::fleet_steer::handle::<D>)
            .layer(DefaultBodyLimit::max(TEXT_BODY_BYTES_MAX)),
        RunnerRoute::Fleets => get(handler::runner::fleets::list::<D>),
        RunnerRoute::FleetTurns => get(handler::runner::fleets::events::<D>),
    }
}
