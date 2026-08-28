//! The whole gate pass, in the order the order itself is a security property.
//!
//! # A recorded gate is read before any policy is
//!
//! A reference means this event was ALREADY parked and a human was already
//! asked. That question outlives the policy that raised it, so it is honoured
//! first. Reading policy first let a mid-flight `config_json` PATCH silently
//! withdraw a question already put to a human: dropping `gates` returned "pass"
//! at the top, and emptying `gates.rules` fell through to auto-approve — either
//! way the parked event ran while its card still sat unanswered. [`route`]
//! carries that ordering as a pure function; this module is the I/O it composes.
//!
//! Cost: one lookup per event, ungated fleets included. It is bought on the
//! path that issues a lease — a whole model run — so it does not register.
//!
//! # The anomaly counter is reached only on a FIRST encounter
//!
//! It is an increment. Re-polling a parked event through it would count one
//! waiting human as N runaway attempts and eventually stop the fleet for being
//! patient. The ordering above is what prevents that, which is why
//! [`Gates::anomaly`] takes no [`RefState`]: by the time it is called, the
//! question is settled.
//!
//! # What the pass never does
//!
//! It writes no `core.fleet_events` row. Every outcome below is a decision the
//! CALLER applies once — the same discipline [`Admission`](crate::lease::Admission)
//! states for the money gates, and for the same reason: a refusal that forgot
//! its row and one that wrote it twice are both unrepresentable when the value
//! is the only thing that decides.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetConfig;

use crate::gate::pause::Trigger;
use crate::gate::pending::{Evaluation, GateRef};
use crate::gate::route::RefState;
use crate::gate::store::Gates;

/// A recorded gate's own read would not answer.
const EVENT_DECISION_UNREADABLE: &str = "gate_decision_read_failed";

/// The recorded-gate lookup would not answer.
const EVENT_LOOKUP_UNREADABLE: &str = "gate_ref_lookup_failed";

/// Why the pass is waiting rather than deciding.
///
/// Three situations that all answer no-work this poll, kept apart because an
/// operator reads them differently: a card was just raised, a card raised
/// earlier is still unanswered, or we could not tell whether one exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// A card was raised for this event on this poll.
    Parked,
    /// A card raised on an earlier poll is still unanswered.
    Pending,
    /// The lookup failed, so raising a second card for an event that may
    /// already hold one would be worse than waiting.
    Unreadable,
}

impl Waiting {
    /// The situation's name, for the line an operator reads.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parked => "parked",
            Self::Pending => "pending",
            Self::Unreadable => "unreadable",
        }
    }
}

/// Why the pass ended the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// A human said no.
    Denied,
    /// The deadline passed with no answer.
    Expired,
}

/// What the gate pass decided about one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing stands in the way; the event runs.
    Pass,
    /// A human owes an answer. Answer no-work and write nothing — the delivery
    /// stays leasable and the next poll re-reads the recorded gate.
    Await(Waiting),
    /// End the event; a human refused it or the question lapsed.
    Refuse(Refused),
    /// A rule or a threshold stopped the whole FLEET.
    ///
    /// The event itself is written nothing and stays leasable, so a resume
    /// re-delivers it. It is the fleet that changed state, not this delivery.
    Killed(Trigger),
    /// A datastore would not answer. Nothing was decided and nothing written.
    Unavailable,
}

/// One event, and everything the pass needs to judge it.
#[derive(Debug, Clone, Copy)]
pub struct Check<'a> {
    /// The fleet whose policy applies.
    pub fleet_id: &'a Uuid7,
    /// The workspace it belongs to.
    pub workspace_id: &'a Uuid7,
    /// The event being judged.
    pub event_id: &'a str,
    /// Its type, which the rules match as the TOOL.
    pub event_type: &'a str,
    /// Who raised it, which the rules match as the ACTION.
    pub actor: &'a str,
    /// Its body, as stored — the conditions' context when it parses.
    pub request_json: &'a str,
    /// The config resolved for this lease.
    pub config: &'a FleetConfig,
}

impl Gates {
    /// Judge one event against its fleet's policy and the gates already raised.
    ///
    /// Answers a [`Verdict`] rather than a `Result` for the reason the module
    /// note gives: every fault reachable here has one correct response, and the
    /// caller has no decision left to make about it.
    pub async fn check(&self, request: Check<'_>, now: UnixMillis) -> Verdict {
        // ONE lookup, and all three states come out of it. `Unreadable` stays
        // distinct from `Absent` because collapsing them is unsafe in both
        // directions — see [`RefState`].
        match self.recorded(request.fleet_id, request.event_id).await {
            // A recorded gate decides, whatever the policy now says.
            Ok(Some(reference)) => self.resolve_recorded(&request, &reference, now).await,
            Ok(None) => {
                self.judge_first_encounter(&request, RefState::Absent, now)
                    .await
            }
            Err(fault) => {
                let fleet = request.fleet_id.as_str();
                let reason = fault.to_string();
                tracing::warn!(
                    event = EVENT_LOOKUP_UNREADABLE,
                    fleet_id = fleet,
                    agentsfleet_event_id = request.event_id,
                    reason,
                    "whether this event already holds a gate could not be read"
                );
                self.judge_first_encounter(&request, RefState::Unreadable, now)
                    .await
            }
        }
    }

    /// What a poll makes of the gate this event already holds.
    async fn resolve_recorded(
        &self,
        request: &Check<'_>,
        reference: &GateRef,
        now: UnixMillis,
    ) -> Verdict {
        match self.evaluate(reference, now).await {
            Ok(Evaluation::Approved) => Verdict::Pass,
            Ok(Evaluation::Denied) => Verdict::Refuse(Refused::Denied),
            Ok(Evaluation::Expired) => Verdict::Refuse(Refused::Expired),
            Ok(Evaluation::Pending) => Verdict::Await(Waiting::Pending),
            // A transient read failure must not deny a gate a human APPROVED.
            // Waiting costs a poll; the other direction throws away an answer.
            Err(fault) => {
                let fleet = request.fleet_id.as_str();
                let reason = fault.to_string();
                tracing::warn!(
                    event = EVENT_DECISION_UNREADABLE,
                    fleet_id = fleet,
                    agentsfleet_event_id = request.event_id,
                    reason,
                    "a recorded gate's decision could not be read; the event waits"
                );
                Verdict::Await(Waiting::Unreadable)
            }
        }
    }
}
