//! What a tool asks `agentsfleetd` through the run's verbs: each call as a
//! value naming no fleet, because the fleet is always the run's.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::trigger_verb::{TriggerKind, TriggerVerifier};

/// One schedules call, as a tool asks it: the fleet is the run's, so none
/// names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleCall<'a> {
    /// Create a schedule for the fleet.
    Create {
        /// The call's number in its run, which names its item.
        call_number: u64,
        /// A five-field cron expression.
        cron: &'a str,
        /// The zone it is read in; UTC when absent.
        timezone: Option<&'a str>,
        /// What the fleet is asked to do when it fires.
        message: &'a str,
        /// Whether it retires after its first fire.
        once: bool,
    },
    /// List every schedule of the fleet.
    List,
    /// Change the named fields of a schedule the fleet made.
    Update {
        /// The call's number in its run, which names its item.
        call_number: u64,
        /// The schedule.
        schedule: &'a Uuid7,
        /// A new expression.
        cron: Option<&'a str>,
        /// A new zone.
        timezone: Option<&'a str>,
        /// A new message.
        message: Option<&'a str>,
        /// Whether it should stop firing.
        paused: Option<bool>,
    },
    /// Delete a schedule the fleet made.
    Delete {
        /// The schedule.
        schedule: &'a Uuid7,
    },
    /// Fire a schedule now.
    Run {
        /// The call's number in its run, which names its item.
        call_number: u64,
        /// The schedule.
        schedule: &'a Uuid7,
    },
    /// Read a schedule's runs, newest first.
    Runs {
        /// The schedule.
        schedule: &'a Uuid7,
        /// How many to read.
        limit: Option<u32>,
        /// The `next_cursor` of the page before.
        starting_after: Option<&'a str>,
    },
}

/// One fleets read, as `fleet_activity` asks it: the workspace is the run's,
/// so none names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetsCall<'a> {
    /// List the workspace's fleets, newest first.
    List {
        /// How many to read.
        limit: Option<u32>,
        /// The `next_cursor` of the page before.
        starting_after: Option<&'a str>,
    },
    /// Read one fleet's events, newest first.
    Events {
        /// The fleet.
        fleet: &'a Uuid7,
        /// How many to read.
        limit: Option<u32>,
        /// The `next_cursor` of the page before.
        starting_after: Option<&'a str>,
    },
}

/// A trigger as `trigger_add` asks for it: the triggers verb's body less the
/// run's fence and the call's item, which the supervisor adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrigger<'a> {
    /// What wakes the fleet.
    pub kind: TriggerKind,
    /// A webhook's source slug.
    pub source: Option<Cow<'a, str>>,
    /// The event names that wake the fleet; every event when absent.
    pub events: Option<Vec<Cow<'a, str>>>,
    /// The repositories whose App events wake the fleet, as `owner/name`.
    pub repositories: Option<Vec<Cow<'a, str>>>,
    /// A webhook's signing recipe, for a source outside the built-in registry.
    pub verifier: Option<TriggerVerifier<'a>>,
    /// The workspace vault key holding a webhook's signing secret.
    pub secret_name: Option<Cow<'a, str>>,
    /// A mention's channel.
    pub channel: Option<Cow<'a, str>>,
}

/// One triggers call, as a tool asks it: the fleet is the run's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerCall<'a> {
    /// Add a trigger to the fleet.
    Add {
        /// The call's number in its run, which names its item.
        call_number: u64,
        /// The trigger.
        trigger: NewTrigger<'a>,
    },
    /// List every trigger of the fleet.
    List,
    /// Remove one trigger of the fleet.
    Remove {
        /// The trigger.
        trigger: &'a Uuid7,
    },
}

/// What one access request did, as `agentsfleetd` answered it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAnswer {
    /// A card now waits on a person for this run's turn: the run ends
    /// waiting, and the person's Approve or Deny resumes it.
    Asked,
    /// The fleet's approved grant already covers the request.
    Covered,
    /// An earlier request's card for the same name still waits on a person.
    Pending,
}
