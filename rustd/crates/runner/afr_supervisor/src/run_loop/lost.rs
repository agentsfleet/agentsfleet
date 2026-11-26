//! A sandbox that ends under its run ends the run.
//!
//! ```text
//!   sandbox-side call ─► slot ─► the built sandbox reads Ended?
//!                                 ├─ no  ─► its executor
//!                                 └─ yes ─► `lost`: the call answers `sandbox_unavailable`,
//!                                           the turn is interrupted, the run ends cut
//! ```
//!
//! The host can end a sandbox the engine still holds running: under memory
//! overcommit its kernel ends one microVM's monitor. Every sandbox-side call
//! reads the sandbox's state before it is lent the executor, so the first
//! call after the loss ends the run instead of failing call after call. The
//! cut drops the run's task transition, so a durable task resumes from its
//! last checkpoint, as a run whose runner was killed does.

use std::sync::OnceLock;

use af_wire::report::FailureClass;
use afr_engine::{Sandbox, SandboxState};
use afr_executor::{Executor, ExecutorSlot, ExecutorSource, Outcome, Unavailable};
use tokio_util::sync::CancellationToken;

use crate::report::Ending;

/// What a run whose sandbox ended under it reports.
const DETAIL_LOST: &str = "the run's sandbox ended while the run went on";
/// The event a run ended by its sandbox's loss is logged under.
const EVENT_LOST: &str = "run_sandbox_lost";

/// The run's slot, read through the sandbox it lends from.
#[derive(Debug)]
pub(super) struct Watched<'w, 'c> {
    slot: &'w ExecutorSlot<'c>,
    cell: &'c OnceLock<Box<dyn Sandbox>>,
    /// Cancelled to end the turn once the sandbox is found ended.
    interrupt: &'w CancellationToken,
    /// Cancelled once, by the call that found the sandbox ended.
    lost: CancellationToken,
    run_id: &'w str,
}

impl<'w, 'c> Watched<'w, 'c> {
    /// `slot`, lending from the sandbox `cell` holds once built, ending the
    /// run `run_id` through `interrupt` when that sandbox ends.
    pub(super) fn new(
        slot: &'w ExecutorSlot<'c>,
        cell: &'c OnceLock<Box<dyn Sandbox>>,
        interrupt: &'w CancellationToken,
        run_id: &'w str,
    ) -> Self {
        Self {
            slot,
            cell,
            interrupt,
            lost: CancellationToken::new(),
            run_id,
        }
    }

    /// `ending`, cut as a loss when a call found the sandbox ended.
    pub(super) fn judged(&self, ending: Ending) -> Ending {
        if self.lost.is_cancelled() {
            ending.cut(FailureClass::TransportLoss, DETAIL_LOST)
        } else {
            ending
        }
    }

    /// Whether the built sandbox reads ended; the first call to find it so
    /// logs it and interrupts the turn.
    fn ended(&self) -> bool {
        let ended = self
            .cell
            .get()
            .is_some_and(|sandbox| sandbox.state() == SandboxState::Ended);
        if ended && !self.lost.is_cancelled() {
            self.lost.cancel();
            self.interrupt.cancel();
            let (run_id, event) = (self.run_id, EVENT_LOST);
            tracing::warn!(
                run_id,
                event,
                "the run's sandbox ended; the run ends with it"
            );
        }
        ended
    }
}

#[async_trait::async_trait]
impl ExecutorSource for Watched<'_, '_> {
    async fn get(&self) -> Outcome<'_> {
        let executor = self.slot.get().await?;
        if self.ended() {
            return Err(Unavailable::Lost);
        }
        Ok(executor)
    }

    fn peek(&self) -> Option<&dyn Executor> {
        self.slot.peek()
    }

    fn is_filled(&self) -> bool {
        self.slot.is_filled()
    }
}
