//! How a pass ends: the terminal row, the tail's closing bracket, and the
//! bytes the runner reads.
//!
//! Split from [`super`] by concern (RULE FLL): that module decides WHETHER an
//! event runs, and this one is what happens once the answer is no — including
//! the assignment read that produces one of those noes. They change
//! for different reasons — a new gate moves the decision, a new `failure_label`
//! moves the ending — and the file cap forced the cut at a seam the two
//! already had.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_core::turn::label;

use super::step::{Step, waited};
use super::{AdmittedFleetWork, RunService};
use crate::error::{Error, Result};
use crate::run::admit::{Declined, Refusal};
use crate::run::answer::{EVENT_ABANDONED, EVENT_REFUSED, no_work};
use crate::run::envelope::Acquired;
use crate::run::installed::Installed;
use crate::run::turn::{DETAIL_RUNS_LAPSED, Delivery, Ended};

/// The no-work reason for a fleet archived between selection and assignment.
const FLEET_ARCHIVED: &str = "the fleet was archived between selection and assignment";

/// A fleet whose unreadable config could not even be recorded as such.
pub(super) const EVENT_CONFIG_REFUSAL_UNRECORDED: &str = "config_refusal_unrecorded";

impl RunService {
    /// The installed fleet behind an assignment, or the answer that ends the pass.
    ///
    /// Three of the four arms end it, which is why this is a
    /// [`Step`](super::step::Step) rather than an `Option`: an archived fleet, an
    /// unreadable document and a datastore outage are different endings, and
    /// the caller should not have to tell them apart to know the pass is over.
    pub(super) async fn resolve_installed(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
    ) -> Result<Step<Installed>> {
        match self.runs.installed_assigned(&acquired.opened).await {
            Ok(Some(installed)) => Ok(Step::Go(installed)),
            // The selection pass filters on status, so reaching here with a
            // fleet that is not active means it was archived in the window
            // between selection and this read. The pass frees the assignment; the
            // archive already cleared the mark.
            Ok(None) => {
                no_work(runner_id, FLEET_ARCHIVED);
                Ok(Step::Stop)
            }
            // One fleet's unreadable document is that fleet's fault and not
            // this runner's, which is the whole of `refuse_unreadable_config`.
            Err(changed) if changed.code() == af_core::error_code::TASK_INVALID_TARGET => {
                self.runs.block_changed_task_setup(acquired).await?;
                no_work(
                    runner_id,
                    "the task's accepted setup no longer fits current authority",
                );
                Ok(Step::Stop)
            }
            Err(unreadable) if unreadable.is_config_permanent() => self
                .refuse_unreadable_config(acquired, runner_id, &unreadable)
                .await
                .map(|()| Step::Stop),
            Err(outage) => Err(outage),
        }
    }

    /// Apply a stop the payer, money or approval gate declined with.
    ///
    /// A wait on a person parks; a refusal ends the event; a retry leaves the
    /// delivery leasable for the next poll.
    pub(super) async fn ended(
        &self,
        acquired: &Acquired,
        declined: Declined,
        runner_id: &Uuid7,
    ) -> Result<Step<AdmittedFleetWork>> {
        match declined {
            Declined::Await(waiting) => Ok(waited(runner_id, waiting)),
            Declined::Refuse(refusal) => self
                .refused(acquired, refusal.label, runner_id, "")
                .await
                .map(|()| Step::Stop),
            Declined::Retry(transient) => {
                no_work(runner_id, transient.at);
                Ok(Step::Stop)
            }
        }
    }

    /// End the event, then answer no-work.
    ///
    /// The refusal is written before the answer. Whether a row MOVED decides
    /// only the tail's closing bracket: an already-terminal row is a
    /// redelivery whose earlier acknowledgement was lost, its watchers already
    /// hold the ending, and the runner is told the same thing either way.
    pub(in crate::run) async fn refused(
        &self,
        acquired: &Acquired,
        label: &'static str,
        runner_id: &Uuid7,
        reason: &str,
    ) -> Result<()> {
        let ended = self.runs.block(acquired, Refusal::labelled(label)).await?;
        self.close_tail(ended).await;
        let runner_id_field = runner_id.as_str();
        let fleet_id_field = acquired.fleet_id.as_str();
        let turn_id_field = acquired.turn_id.as_str();
        {
            let (event, runner_id, fleet_id, agentsfleet_turn_id, message) = (
                &EVENT_REFUSED,
                &runner_id_field,
                &fleet_id_field,
                &turn_id_field,
                format_args!("the event was ended at a gate"),
            );
            tracing::warn!(
                message,
                event,
                runner_id,
                fleet_id,
                agentsfleet_turn_id,
                label,
                reason
            );
        };
        no_work(runner_id, label);
        Ok(())
    }

    /// End an event whose runs kept lapsing, then answer no-work.
    ///
    /// Logged under its own event, apart from a gate's refusal. The fleet's
    /// next event is assignable the moment this commits: the ending settles
    /// the turn that was holding the queue's head.
    pub(super) async fn abandoned(&self, acquired: &Acquired, runner_id: &Uuid7) -> Result<()> {
        let ended = self.runs.abandon(acquired).await?;
        self.close_tail(ended).await;
        let runner_id_field = runner_id.as_str();
        let fleet_id_field = acquired.fleet_id.as_str();
        let turn_id_field = acquired.turn_id.as_str();
        let earlier_runs = acquired.earlier_runs;
        {
            let (event, runner_id, fleet_id, agentsfleet_turn_id, label, reason, message) = (
                &EVENT_ABANDONED,
                &runner_id_field,
                &fleet_id_field,
                &turn_id_field,
                &label::RUN_EXPIRED,
                &DETAIL_RUNS_LAPSED,
                format_args!("the event was ended after its runs kept lapsing"),
            );
            tracing::warn!(
                message,
                event,
                runner_id,
                fleet_id,
                agentsfleet_turn_id,
                label,
                earlier_runs,
                reason
            );
        };
        no_work(runner_id, label::RUN_EXPIRED);
        Ok(())
    }

    /// The tail's closing bracket for an event this pass ended. A row an
    /// earlier pass ended already has its bracket, so nothing is published.
    async fn close_tail(&self, ended: Ended) {
        if let Ended::Now(closed) = ended {
            let publication = self.runs.publish_completion(&closed, None).await;
            afd_bus::observe_notification(
                &afd_bus::Subject::ThreadItems(closed.row.fleet_id.clone()),
                publication,
            );
        }
    }

    /// One fleet's unreadable configuration ends THAT fleet's event, and
    /// nothing else.
    ///
    /// # What this corrects
    ///
    /// The fault used to leave the pull path as an error, so the poll answered
    /// 500. The runner had done nothing wrong, and a runner does not poll one
    /// fleet — a rotation visits every partition — so a single fleet carrying
    /// a document this daemon cannot parse refused every runner on the
    /// deployment, for every fleet. One unreadable document is one fleet's
    /// problem, which is the decision recorded in the spec's Discovery log.
    ///
    /// # Why the ending is terminal rather than a retry
    ///
    /// [`Error::is_config_permanent`] already classifies it, and the reasoning
    /// is that module's: a document that will not parse does not become
    /// parseable by being read again, so every poll would re-read the same
    /// bytes and leave the delivery leasable forever. The terminal row is what
    /// puts the fleet in front of a human, and the runner is answered the
    /// documented no-work rather than an error it would count toward its own
    /// self-termination ceiling.
    ///
    /// # Why the narrative row is opened first
    ///
    /// [`Runs::block`](crate::run::store::Runs::block) is an UPDATE
    /// guarded on `received`. A refusal written with no open row moves nothing,
    /// acknowledges the entry anyway, and leaves an operator with a fleet that
    /// quietly drops its work. Opening it here is the same pair of brackets
    /// every other refusal writes — the tail's watchers see the run open and
    /// close, rather than a run that never existed.
    pub(super) async fn refuse_unreadable_config(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
        fault: &Error,
    ) -> Result<()> {
        let Logged {
            error_code: config_error_code,
            reason,
        } = fault.logged();
        match self.end_unreadable(acquired, runner_id, &reason).await {
            Ok(answer) => Ok(answer),
            // Logged, never propagated, and the reasoning is this module's
            // already: the terminal-redelivery acknowledgement a few lines
            // upstream is swallowed for the same reason, because "failing the
            // run would refuse a runner that has done nothing wrong". It
            // applies with more force here. The rows being written belong to
            // ANOTHER fleet — one this runner only met because a rotation
            // sampled its partition — so a failure writing them is doubly not
            // this runner's to answer for. The entry stays pending and a later
            // poll runs this path again, which is the same answer one turn
            // later.
            //
            // This is not hypothetical tidying. The write races whatever else
            // holds the deployment: a fleet row removed between the assignment and
            // this insert takes its foreign keys with it, and the poll that
            // happened to be ending that fleet's event answered `500 Database
            // error` to a runner with no stake in any of it — which is the
            // failure this whole path exists to stop, wearing a different
            // error code.
            Err(unrecorded) => {
                let fleet_id_field = acquired.fleet_id.as_str();
                let turn_id_field = acquired.turn_id.as_str();
                let Logged {
                    error_code,
                    reason: unrecorded_reason,
                } = unrecorded.logged();
                {
                    let (fleet_id, agentsfleet_turn_id, reason, config_reason, event, message) = (
                        &fleet_id_field,
                        &turn_id_field,
                        &unrecorded_reason,
                        &reason,
                        &EVENT_CONFIG_REFUSAL_UNRECORDED,
                        format_args!(
                            "a fleet with an unreadable config could not be ended; it \
                     will be assigned again"
                        ),
                    );
                    tracing::warn!(
                        message,
                        error_code,
                        fleet_id,
                        agentsfleet_turn_id,
                        reason,
                        config_error_code,
                        config_reason,
                        event
                    );
                };
                no_work(runner_id, label::CONFIG_UNREADABLE);
                Ok(())
            }
        }
    }

    /// The durable half of [`Self::refuse_unreadable_config`].
    ///
    /// Separated so the caller can answer the runner whether or not this
    /// succeeded, without a `match` arm deciding it in the middle of the
    /// write sequence.
    async fn end_unreadable(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
        reason: &str,
    ) -> Result<()> {
        let received = acquired.opened.received;
        if received.delivery == Delivery::First {
            self.runs
                .publish_received(acquired, received.opened_at, received.counters)
                .await;
        }
        self.refused(acquired, label::CONFIG_UNREADABLE, runner_id, reason)
            .await
    }
}
