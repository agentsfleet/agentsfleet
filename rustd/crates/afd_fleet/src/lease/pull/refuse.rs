//! How a pass ends: the terminal row, the tail's closing bracket, and the
//! bytes the runner reads.
//!
//! Split from [`super`] by concern (RULE FLL): that module decides WHETHER an
//! event runs, and this one is what happens once the answer is no — including
//! the claim read that produces one of those noes. They change
//! for different reasons — a new gate moves the decision, a new `failure_label`
//! moves the ending — and the file cap forced the cut at a seam the two
//! already had.

use afd_core::clock::UnixMillis;
use afd_core::event::label;
use afd_core::id::Uuid7;

use super::step::{AWAITING_APPROVAL, Step};
use super::{Admission2, Plane};
use crate::error::{Error, Result};
use crate::lease::admit::{Admission, Refusal};
use crate::lease::answer::{EVENT_REFUSED, no_work};
use crate::lease::envelope::Acquired;
use crate::lease::event::Delivery;
use crate::lease::installed::Installed;

/// A fleet whose unreadable config could not even be recorded as such.
const EVENT_CONFIG_REFUSAL_UNRECORDED: &str = "config_refusal_unrecorded";

impl Plane {
    /// The installed fleet behind a claim, or the answer that ends the pass.
    ///
    /// Three of the four arms end it, which is why this is a
    /// [`Step`](super::step::Step) rather than an `Option`: a paused fleet, an
    /// unreadable document and a datastore outage are different endings, and
    /// the caller should not have to tell them apart to know the pass is over.
    pub(super) async fn resolve_installed(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Step<Installed>> {
        match self.leases.installed(&acquired.fleet_id).await {
            Ok(Some(installed)) => Ok(Step::Go(installed)),
            // The selection pass filters on status, so reaching here with a
            // stopped fleet means an operator paused it in the window between
            // selection and this read. The claim lapses on its own.
            Ok(None) => Ok(Step::Stop(no_work(
                runner_id,
                "the fleet stopped between selection and claim",
            )?)),
            // One fleet's unreadable document is that fleet's fault and not
            // this runner's, which is the whole of `refuse_unreadable_config`.
            Err(unreadable) if unreadable.is_config_permanent() => self
                .refuse_unreadable_config(acquired, runner_id, &unreadable, now)
                .await
                .map(Step::Stop),
            Err(outage) => Err(outage),
        }
    }

    /// Apply a gate's stop, whatever kind it was.
    pub(super) async fn stopped(
        &self,
        acquired: &Acquired,
        stop: Admission,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<Step<Admission2>> {
        let answer = match stop {
            Admission::Refuse(refusal) => {
                self.refused(acquired, refusal.label, runner_id, refusal.detail, now)
                    .await?
            }
            Admission::Retry(transient) => no_work(runner_id, transient.at)?,
            Admission::Await(_waiting) => no_work(runner_id, AWAITING_APPROVAL)?,
            // `of_gate` answers `None` for a pass, so this arm is the enum
            // being exhaustive rather than a state that occurs.
            Admission::Admit(_) => no_work(runner_id, "a passing gate cannot also stop")?,
        };
        Ok(Step::Stop(answer))
    }

    /// End the event, then answer no-work.
    ///
    /// The refusal is written before the answer. Whether a row MOVED decides
    /// only the tail's closing bracket: an already-terminal row is a
    /// redelivery whose earlier acknowledgement was lost, its watchers already
    /// hold the ending, and the runner is told the same thing either way.
    pub(in crate::lease) async fn refused(
        &self,
        acquired: &Acquired,
        label: &'static str,
        runner_id: &Uuid7,
        reason: &str,
        now: UnixMillis,
    ) -> Result<String> {
        let ended = self
            .leases
            .block(
                &acquired.fleet_id,
                &acquired.event_id,
                Refusal { label, detail: "" },
                now,
            )
            .await?;
        if let crate::lease::event::Ended::Now(closed) = ended {
            self.leases.publish_completion(&closed, None).await;
        }
        self.leases
            .acknowledge(&acquired.fleet_id, &acquired.receipt)
            .await?;
        let runner_id_field = runner_id.as_str();
        let fleet_id_field = acquired.fleet_id.as_str();
        let event_id_field = acquired.event_id.as_str();
        tracing::warn!(
            event = EVENT_REFUSED,
            runner_id = runner_id_field,
            fleet_id = fleet_id_field,
            agentsfleet_event_id = event_id_field,
            label,
            reason,
            "the event was ended at a gate"
        );
        no_work(runner_id, label)
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
    /// [`Leases::block`](crate::lease::store::Leases::block) is an UPDATE
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
        now: UnixMillis,
    ) -> Result<String> {
        let reason = fault.to_string();
        match self.end_unreadable(acquired, runner_id, &reason, now).await {
            Ok(answer) => Ok(answer),
            // Logged, never propagated, and the reasoning is this module's
            // already: the terminal-redelivery acknowledgement a few lines
            // upstream is swallowed for the same reason, because "failing the
            // lease would refuse a runner that has done nothing wrong". It
            // applies with more force here. The rows being written belong to
            // ANOTHER fleet — one this runner only met because a rotation
            // sampled its partition — so a failure writing them is doubly not
            // this runner's to answer for. The entry stays pending and a later
            // poll runs this path again, which is the same answer one turn
            // later.
            //
            // This is not hypothetical tidying. The write races whatever else
            // holds the deployment: a fleet row removed between the claim and
            // this insert takes its foreign keys with it, and the poll that
            // happened to be ending that fleet's event answered `500 Database
            // error` to a runner with no stake in any of it — which is the
            // failure this whole path exists to stop, wearing a different
            // error code.
            Err(unrecorded) => {
                let code = unrecorded.code().as_str();
                let fleet_id_field = acquired.fleet_id.as_str();
                let event_id_field = acquired.event_id.as_str();
                let unrecorded_reason = unrecorded.to_string();
                tracing::warn!(
                    error_code = code,
                    fleet_id = fleet_id_field,
                    agentsfleet_event_id = event_id_field,
                    reason = unrecorded_reason,
                    config_reason = reason,
                    event = EVENT_CONFIG_REFUSAL_UNRECORDED,
                    "a fleet with an unreadable config could not be ended; it \
                     will be offered again"
                );
                no_work(runner_id, label::CONFIG_UNREADABLE)
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
        now: UnixMillis,
    ) -> Result<String> {
        let received = self.leases.record_received(acquired, now).await?;
        if received.delivery == Delivery::First {
            self.leases
                .publish_received(acquired, now, received.counters)
                .await;
        }
        self.refused(acquired, label::CONFIG_UNREADABLE, runner_id, reason, now)
            .await
    }
}
