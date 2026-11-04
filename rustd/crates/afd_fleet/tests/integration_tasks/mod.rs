//! Complete native task transitions under the fleet's ordinary lease and queue.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "scenario prerequisites fail loudly")]

mod bindings;
mod capacity;
mod controls;
mod decisions;
mod exhaustion;
mod fixture;
mod qualification;
mod read_deadline;
mod stored_state;
