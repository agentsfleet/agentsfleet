//! How a pass ends: the terminal row, the tail's closing bracket, and the
//! bytes the runner reads.
//!
//! Split from [`super`] by concern (RULE FLL): that module decides WHETHER an
//! event runs, and this one is what happens once the answer is no — including
//! the claim read that produces one of those noes. They change
//! for different reasons — a new gate moves the decision, a new `failure_label`
//! moves the ending — and the file cap forced the cut at a seam the two
//! already had.

use std::ops::ControlFlow;

use afd_core::error_code::{Coded as _, Logged};
use afd_core::event::label;
use afd_core::id::Uuid7;

use super::held::{Ending, Step, waited};
use super::{Admission2, Plane};
use crate::error::{Error, Result};
use crate::lease::admit::{Declined, Refusal};
use crate::lease::affinity::Backoff;
use crate::lease::answer::{EVENT_REFUSED, no_work};
use crate::lease::envelope::Acquired;
use crate::lease::installed::Installed;

/// The no-work reason for a fleet paused between selection and claim.
const FLEET_STOPPED: &str = "the fleet stopped between the claim and its read";

/// A fleet whose unreadable config could not even be recorded as such.
pub(super) const EVENT_CONFIG_REFUSAL_UNRECORDED: &str = "config_refusal_unrecorded";

impl Plane {
    /// The installed fleet behind a claim, or the answer that ends the pass.
    ///
    /// Three of the four arms end it, which is why this is a
    /// [`Step`](super::held::Step) rather than an `Option`: a paused fleet, an
    /// unreadable document and a datastore outage are different endings, and
    /// the caller should not have to tell them apart to know the pass is over.
    pub(super) async fn resolve_installed(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
    ) -> Result<Step<Installed>> {
        match self.leases.installed(&acquired.fleet_id).await {
            Ok(Some(installed)) => Ok(ControlFlow::Continue(installed)),
            // The claim filters on status, so reaching here with a stopped
            // fleet means an operator paused it in the window between the
            // claim and this read. The pass frees the claim at once; the claim
            // skips a paused fleet until it resumes.
            Ok(None) => no_work(runner_id, FLEET_STOPPED)
                .map(|answer| ControlFlow::Break(Ending::Stop(answer, Backoff::None))),
            // One fleet's unreadable document is that fleet's fault and not
            // this runner's, which is the whole of `refuse_unreadable_config`.
            Err(unreadable) if unreadable.is_config_permanent() => self
                .refuse_unreadable_config(acquired, runner_id, &unreadable)
                .await
                .map(ControlFlow::Break),
            Err(outage) => Err(outage),
        }
    }

    /// Apply a stop the payer, money or approval gate declined with.
    ///
    /// A wait on a person parks; a refusal ends the event; a retry leaves the
    /// delivery leasable once the retry backoff has passed.
    pub(super) async fn ended(
        &self,
        acquired: &Acquired,
        declined: Declined,
        runner_id: &Uuid7,
    ) -> Result<Step<Admission2>> {
        match declined {
            Declined::Await(waiting) => waited(runner_id, waiting),
            Declined::Refuse(refusal) => self
                .refused(acquired, refusal.label, runner_id, refusal.detail)
                .await
                .map(|answer| ControlFlow::Break(Ending::Ended(answer))),
            Declined::Retry(transient) => no_work(runner_id, transient.at)
                .map(|answer| ControlFlow::Break(Ending::Stop(answer, Backoff::Retry))),
        }
    }

    /// End the event, then answer no-work.
    ///
    /// The refusal is written before the answer, in the one statement that
    /// also settles the admission and frees the slot. Whether a row MOVED
    /// decides only the tail's closing bracket: an already-terminal row is a
    /// redelivery whose earlier acknowledgement was lost, its watchers already
    /// hold the ending, and the runner is told the same thing either way.
    pub(in crate::lease) async fn refused(
        &self,
        acquired: &Acquired,
        label: &'static str,
        runner_id: &Uuid7,
        reason: &str,
    ) -> Result<String> {
        let ended = self
            .leases
            .block(acquired, Refusal { label, detail: "" })
            .await?;
        if let crate::lease::event::Ended::Now(closed) = ended {
            self.leases.publish_completion(&closed, None).await;
        }
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
    /// # Why the narrative row is already open
    ///
    /// [`Leases::block`](crate::lease::store::Leases::block) is an UPDATE
    /// guarded on `received`. A refusal written with no open row moves nothing,
    /// acknowledges the entry anyway, and leaves an operator with a fleet that
    /// quietly drops its work. The claim opened the row before this read ran,
    /// so this is the same pair of brackets every other refusal writes — the
    /// tail's watchers see the run open and close, rather than a run that
    /// never existed.
    pub(super) async fn refuse_unreadable_config(
        &self,
        acquired: &Acquired,
        runner_id: &Uuid7,
        fault: &Error,
    ) -> Result<Ending> {
        let Logged {
            error_code: config_error_code,
            reason,
        } = fault.logged();
        match self
            .refused(acquired, label::CONFIG_UNREADABLE, runner_id, &reason)
            .await
        {
            Ok(answer) => Ok(Ending::Ended(answer)),
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
                let fleet_id_field = acquired.fleet_id.as_str();
                let event_id_field = acquired.event_id.as_str();
                let Logged {
                    error_code,
                    reason: unrecorded_reason,
                } = unrecorded.logged();
                tracing::warn!(
                    error_code,
                    fleet_id = fleet_id_field,
                    agentsfleet_event_id = event_id_field,
                    reason = unrecorded_reason,
                    config_error_code,
                    config_reason = reason,
                    event = EVENT_CONFIG_REFUSAL_UNRECORDED,
                    "a fleet with an unreadable config could not be ended; it \
                     will be offered again"
                );
                no_work(runner_id, label::CONFIG_UNREADABLE)
                    .map(|answer| Ending::Stop(answer, Backoff::Retry))
            }
        }
    }
}
