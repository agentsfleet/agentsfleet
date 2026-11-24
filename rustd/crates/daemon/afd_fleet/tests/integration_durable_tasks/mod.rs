//! Complete native task transitions under the fleet's ordinary run and queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "scenario prerequisites fail loudly")]

mod bindings;
mod capacity;
mod controls;
pub(crate) mod decisions;
mod exhaustion;
mod fixture;
mod fold_barrier;
mod gained_durable_task;
mod input_answers;
mod input_receipts;
mod inputs_page;
mod qualification;
mod read_deadline;
mod receipt_branches;
mod replay_reads;
mod report_evidence;
mod report_limits;
mod report_settlement;
mod stored_state;
mod task_edits;
mod task_inputs;
mod task_runs;
mod task_tools;
mod tenant_placement;
mod transition_events;
