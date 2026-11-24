//! A fleet as its workspace reads and changes it: a name, a model, its network
//! toggles, what wakes it, what it may access, and its tasks.

mod collection;
mod durable_task;
mod input;
mod memory;
mod output;
mod trigger;

pub use collection::{FleetSummary, FleetsResponse};
pub use durable_task::{
    DurableTaskControlState, DurableTaskResponse, DurableTaskSummary, DurableTasksResponse,
    PatchDurableTaskRequest,
};
pub use input::{CreateFleetRequest, PatchFleetRequest, PatchStatus};
pub use memory::{MemoryAccess, MemoryAccessRequest};
pub use output::{CreatedFleet, FirstTurn, FirstTurnDisposition, FleetResponse};
pub use trigger::{Trigger, TriggersResponse};
