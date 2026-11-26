//! The assignment a pass holds once it wins a fleet, and the one place it is let
//! go.
//!
//! Every ending that issues no run frees the assignment — a refusal, a retry, a
//! park, a fault — and it is freed here, after the pass has its answer, so a
//! stop added later cannot forget to. Each of those endings used to return
//! with the assignment still held, and the fleet's next event waited out the
//! assignment's whole lifetime behind an answer that had already been given.
//!
//! An assignment the issue refused for room, time or a moved placement choice
//! also rings its fleet's wake once freed, as a released unsent run does: the
//! scheduler runs no timer, so a turn nobody rings waits for whichever pass
//! reaches it next.

use std::time::Instant;

use af_core::id::Uuid7;
use af_observability::verb_span::{AssignStage, VerbClock};
use af_wire::run::RunPayload;
use jiff::Timestamp;

use super::RunService;
use super::step::{Assigned, Step};
use crate::error::Result;
use crate::run::AssignedRun;
use crate::run::affinity::Fence;
use crate::run::envelope::Acquired;

/// What a pass must let go of when it issues no run.
struct Held {
    fleet_id: Uuid7,
    fence: Fence,
    turn_id: Uuid7,
}

impl RunService {
    /// Runs an assigned pass, marking the start check, the issue and the assignment's
    /// encoding on `clock`. The issue ends where `emit` is called, or where the
    /// delivery returns without calling it, so a refused assignment shows `issue`
    /// and no `encode`.
    pub(super) async fn run_assigned_with<T, F>(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: Timestamp,
        began: Instant,
        clock: &mut VerbClock<AssignStage>,
        emit: &mut F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, AssignedRun) -> T + Send,
    {
        let held = Held {
            fleet_id: acquired.fleet_id.clone(),
            fence: acquired.fence,
            turn_id: acquired.turn_id.clone(),
        };
        clock.identify(held.turn_id.as_str(), held.fleet_id.as_str());
        let checked = self.check_assigned(acquired, runner_id, now).await;
        clock.mark(AssignStage::Admit);
        let pass = match checked.map(Step::proceed) {
            Ok(Ok(checked)) => {
                let delivered = {
                    let mut timed = |run: RunPayload<'_>, assigned: AssignedRun| {
                        clock.mark(AssignStage::Issue);
                        let encoded = emit(run, assigned);
                        clock.mark(AssignStage::Encode);
                        encoded
                    };
                    self.deliver_with(runner_id, checked, now, began, &mut timed)
                        .await
                };
                // The issue ends where `emit` is called. When it never is (a
                // refusal or a fault), it ends here, so that attempt's time is
                // not left unexplained; a stage already marked keeps its first
                // reading.
                clock.mark(AssignStage::Issue);
                delivered
            }
            Ok(Err(ended)) => Ok(ended),
            Err(fault) => Err(fault),
        };
        match pass {
            Ok(Step::Go(Assigned(answer))) => Ok(Some(answer)),
            Ok(Step::Stop) => {
                self.let_go(&held).await;
                Ok(None)
            }
            Ok(Step::Lost) => {
                // The turn is still its fleet's head, and nothing else owes it
                // a pass: once freed, it is the next pass's hint.
                if self.let_go(&held).await {
                    afd_bus::wake::publish(&self.runs.streams(), held.fleet_id).await;
                }
                Ok(None)
            }
            Err(fault) => {
                self.let_go(&held).await;
                Err(fault)
            }
        }
    }

    /// Frees the assignment through the one best-effort release every
    /// run-less ending shares, and answers whether it did.
    async fn let_go(&self, held: &Held) -> bool {
        self.runs.let_go(&held.fleet_id, held.fence).await
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use af_core::test_util::trace::Capture;

    use crate::run::affinity::EVENT_ASSIGNMENT_RELEASE_FAILED;
    use crate::run::test_dead;

    /// A fault after the assignment reaches the caller as the fault, and the assignment
    /// is still let go — here the release is refused too, so it is logged
    /// under the fault's own code and the assignment lapses at its expiry instead.
    #[tokio::test]
    async fn should_raise_the_fault_and_log_a_release_the_datastore_refuses() {
        let log = Capture::install();
        // Installed, so the pass gets past the row the assignment carried and
        // reaches its first read after it, which a dead datastore refuses.
        let acquired = test_dead::acquired();
        let fleet = acquired.fleet_id.clone();

        let fault = test_dead::plane()
            .run_assigned_with(
                acquired,
                &test_dead::id(9),
                test_dead::AT,
                std::time::Instant::now(),
                &mut af_observability::verb_span::VerbClock::untimed(),
                &mut |_run, _metadata| (),
            )
            .await
            .expect_err("a read after the assignment with no datastore is a fault, not a decision");

        // The first read after the assignment is the credential plane's, because the
        // assignment carries the fleet row; whichever read it is, its fault is the
        // caller's, and the release below carries the same code.
        let line = log.only(EVENT_ASSIGNMENT_RELEASE_FAILED).fields;
        assert_eq!(
            line.get("fleet_id").map(String::as_str),
            Some(fleet.as_str())
        );
        assert_eq!(
            line.get("error_code").map(String::as_str),
            Some(fault.code().as_str()),
            "the refused release carries the same outage's code"
        );
    }
}
