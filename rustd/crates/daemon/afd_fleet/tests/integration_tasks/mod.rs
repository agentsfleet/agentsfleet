//! Complete native task transitions under the fleet's ordinary lease and queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "scenario prerequisites fail loudly")]

mod bindings;
mod capacity;
mod controls;
mod decisions;
mod exhaustion;
mod fixture;
mod fold_barrier;
mod input_answers;
mod input_receipts;
mod qualification;
mod read_deadline;
mod replay_reads;
mod report_limits;
mod report_settlement;
mod stored_state;
