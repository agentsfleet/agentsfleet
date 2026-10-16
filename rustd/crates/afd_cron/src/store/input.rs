//! Schedule intent and fire-target values shared by the store operations.

use crate::model::{DesiredStatus, Source};
use afd_core::id::Uuid7;

/// What a schedule is created from.
///
/// A struct rather than eight positional arguments because four of them are
/// `&str` a call site could transpose without the compiler noticing.
#[derive(Debug, Clone, Copy)]
pub struct NewSchedule<'a> {
    /// The fleet the schedule wakes.
    pub fleet: &'a Uuid7,
    /// Who asked for it.
    pub source: Source,
    /// The key the external scheduler will know it by, or `None` for the
    /// schedule's own identifier.
    ///
    /// `None` is unique by construction, which a fleet needs: one run may
    /// create two schedules in the same instant, and the key is unique per
    /// fleet.
    pub source_key: Option<&'a str>,
    /// The expression, already through [`crate::validate::cron`].
    pub cron: &'a str,
    /// The zone, already through [`crate::validate::timezone`].
    pub timezone: &'a str,
    /// The message, already through [`crate::validate::message`].
    pub message: &'a str,
    /// Whether it retires after its first fire.
    pub once: bool,
}

/// What an edit changes, field by field.
///
/// Every field is optional and an absent one is left alone — the `COALESCE`
/// half of [`sql::CLAIM_MUTATION`]. A struct of `Option` rather than a full
/// replacement value, because a partial edit expressed as a whole row would
/// make every caller read before it writes and race its own read.
#[derive(Debug, Clone, Copy, Default)]
pub struct Change<'a> {
    /// A new expression, or the one already stored.
    pub cron: Option<&'a str>,
    /// A new zone, or the one already stored.
    pub timezone: Option<&'a str>,
    /// A new message, or the one already stored.
    pub message: Option<&'a str>,
    /// A new intent, or the one already stored.
    pub desired_status: Option<DesiredStatus>,
}

/// Where a fire is going, once a signed callback resolved to it.
#[derive(Debug, Clone)]
pub struct FireTarget {
    /// The fleet to wake.
    pub fleet: Uuid7,
    /// The workspace the event is stamped with.
    pub workspace: Uuid7,
    /// What the fleet is asked to do.
    pub message: String,
    /// Whether the schedule retires once this fire is admitted.
    pub once: bool,
    /// What the operator wants this schedule to be doing.
    pub desired_status: DesiredStatus,
    /// What the fleet's own row says it is doing.
    pub fleet_status: String,
}
