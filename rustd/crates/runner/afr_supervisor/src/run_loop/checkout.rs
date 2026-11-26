//! The fleet's bound repositories, checked out on the host and imported into
//! the run's workspace, so the token that fetched them never enters the
//! sandbox (`crate::workspace_clone`).
//!
//! What can be checked without a sandbox is checked before the model starts,
//! and refuses the run as it would every time: the plan. The checkout itself
//! runs in the run's build, beside the turn.

use af_core::error_code::{Coded, Logged};
use af_core::id::Uuid7;
use afr_engine::Sandbox;
use afr_executor::Unavailable;
use afr_tools::sandbox::{CREDENTIAL_GITHUB, Checkout, checkouts};
use tokio_util::sync::CancellationToken;

use super::Run;
use super::build::Ended;
use crate::credentials;
use crate::error;
use crate::report::Ending;
use crate::workspace_clone::Request;

/// What a run whose repositories would not check out reports.
const DETAIL_CHECKOUT: &str = "a bound repository could not be checked out into the workspace";
const EVENT_CHECKOUT_REFUSED: &str = "repository_checkout_refused";
const EVENT_CHECKOUT_STARTED: &str = "repository_checkout_started";
const EVENT_CHECKOUT_COMPLETED: &str = "repository_checkout_completed";
const EVENT_CHECKOUT_FAILED: &str = "repository_checkout_failed";
const EVENT_CHECKOUT_STOPPED: &str = "repository_checkout_stopped";

/// What a run checks out: its repositories, under its workspace's id.
#[derive(Debug)]
pub(super) struct Plan<'p> {
    pub(super) wanted: Vec<Checkout<'p>>,
    scope: Uuid7,
}

impl Run<'_> {
    /// The run's checkout, read off its policy before any model request:
    /// none when no sandbox tool works in a repository. A binding no checkout
    /// can serve, or a workspace id that is not one, ends the run at
    /// startup. Refusals use the same owned ending as workspace admission.
    pub(super) fn plan<'p>(
        &'p self,
        prepared: Option<&'p afr_agent::PreparedDurableTask>,
    ) -> Result<Option<Plan<'p>>, Box<Ending>> {
        let mut wanted =
            checkouts(&self.payload.policy).map_err(|refusal| Box::new(self.unready(&refusal)))?;
        if let Some(task) =
            prepared.and_then(|prepared| prepared.inputs.current_durable_task.as_ref())
        {
            match &task.target {
                af_wire::durable_task::DurableTaskTarget::ApiOnly => wanted.clear(),
                af_wire::durable_task::DurableTaskTarget::ExistingPr {
                    repository, branch, ..
                } => {
                    wanted.retain(|checkout| checkout.repository == repository);
                    let [checkout] = wanted.as_mut_slice() else {
                        return Err(Box::new(self.unready(&error::config(
                            "native task repository is outside checkout policy",
                        ))));
                    };
                    checkout.base = branch;
                }
            }
        }
        if wanted.is_empty() {
            return Ok(None);
        }
        // The workspace id becomes a directory of the mirror's path, so it is
        // read as an identifier before it is joined onto anything.
        let scope = Uuid7::parse(&self.payload.event.workspace_id)
            .map_err(|refusal| Box::new(self.unready(&refusal)))?;
        Ok(Some(Plan { wanted, scope }))
    }

    /// Checks `plan`'s repositories out into `sandbox`'s workspace. A cancelled
    /// `stop` ends the token's mint where it stands, interrupts a fetch, and
    /// is read between repositories.
    ///
    /// # Errors
    /// [`Ended`]: the stop, or what failed: a token the daemon would not
    /// mint, or a repository that would not check out or import.
    pub(super) async fn check_out(
        &self,
        sandbox: &dyn Sandbox,
        plan: &Plan<'_>,
        stop: &CancellationToken,
    ) -> Result<(), Ended> {
        let minting = credentials::mint(&self.lessee.plane, &self.ids.run, CREDENTIAL_GITHUB, None);
        // A mint dropped mid-call leaves a short-lived token unused; it lapses.
        let token = tokio::select! {
            minted = minting => minted.map_err(|failure| unfinished(&failure))?,
            () = stop.cancelled() => return Err(Ended::Stopped),
        };
        each_until_stopped(&plan.wanted, stop, |checkout| {
            let request = Request {
                scope: plan.scope.as_str(),
                checkout: *checkout,
                token: token.expose(),
            };
            async move {
                self.check_out_one(sandbox, request, stop)
                    .await
                    .map_err(|failure| unfinished(&failure))
            }
        })
        .await
    }

    /// Checks out one repository through a stage of its own, logging it
    /// from start to end.
    async fn check_out_one(
        &self,
        sandbox: &dyn Sandbox,
        request: Request<'_>,
        stop: &CancellationToken,
    ) -> error::Result<()> {
        let run_id = self.ids.run.as_str();
        let repository = request.checkout.repository;
        let event = EVENT_CHECKOUT_STARTED;
        tracing::info!(run_id, repository, event);
        let landed = match self.lessee.staging.stage(sandbox.id()) {
            Ok(staged) => {
                let mirrors = &self.lessee.mirrors;
                mirrors
                    .land(request, &staged, sandbox.executor(), stop)
                    .await
            }
            Err(failure) => Err(failure),
        };
        match landed {
            Ok(fetched) => {
                let fetched = fetched.as_str();
                let event = EVENT_CHECKOUT_COMPLETED;
                tracing::info!(run_id, repository, fetched, event);
                Ok(())
            }
            Err(failure) => {
                checkout_ended(run_id, repository, &failure, stop.is_cancelled());
                Err(failure)
            }
        }
    }

    /// Logs why the run's repositories could not be checked out, and ends
    /// it at startup.
    fn unready(&self, failure: &impl Coded) -> Ending {
        self.refuse(failure, EVENT_CHECKOUT_REFUSED, DETAIL_CHECKOUT)
    }
}

/// Logs a checkout of `repository` that did not finish: one the stop ended
/// is no failure, and is logged at debug as stopped.
fn checkout_ended(run_id: &str, repository: &str, failure: &impl Coded, stopped: bool) {
    let Logged { error_code, reason } = failure.logged();
    if stopped {
        let event = EVENT_CHECKOUT_STOPPED;
        tracing::debug!(run_id, repository, reason, event);
    } else {
        let event = EVENT_CHECKOUT_FAILED;
        tracing::warn!(error_code, run_id, repository, reason, event);
    }
}

/// Runs `work` on each of `items` in order, reading `stop` before each one.
/// A stop ends the run between items and never inside one, so no checkout is
/// dropped half written, and none starts once the turn has ended.
async fn each_until_stopped<T, Work, Done>(
    items: impl IntoIterator<Item = T>,
    stop: &CancellationToken,
    mut work: Work,
) -> Result<(), Ended>
where
    Work: FnMut(T) -> Done,
    Done: Future<Output = Result<(), Ended>>,
{
    for item in items {
        if stop.is_cancelled() {
            return Err(Ended::Stopped);
        }
        work(item).await?;
    }
    Ok(())
}

/// A checkout the build could not finish, as its sandbox-side calls read it.
fn unfinished(failure: &impl Coded) -> Ended {
    Ended::Failed(Unavailable::Checkout, failure.logged())
}

#[cfg(test)]
#[path = "tests/checkout.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/checkout_failures.rs"]
mod failure_tests;
