//! Why a create was refused, and what each refusal tells a caller.
//!
//! Refusals rather than errors, for the reason [`crate::error`] gives: an
//! operator or a fleet hit a bound, and nothing in this daemon failed. The code
//! and the sentence are decided here, together, so the tenant surface and the
//! runner's verb answer one refusal in one spelling.

use afd_core::error_code::{self, ErrorCode};

/// The sentence a fleet that is not in the caller's workspace earns.
pub const DETAIL_NOT_FOUND: &str = "No schedule with that identifier belongs to this fleet.";

/// The sentence a fleet at its schedule ceiling earns.
pub const DETAIL_TOO_MANY: &str = "This fleet already holds as many schedules as it may.";

/// The sentence a fleet at its own cap earns.
pub const DETAIL_FLEET_CAP: &str =
    "This fleet already holds as many schedules as it may create itself.";

/// The sentence a duplicate upstream key earns.
pub const DETAIL_DUPLICATE: &str = "This fleet already has a schedule under that key.";

/// Why a create was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The fleet is not in the workspace the caller was proven in.
    ///
    /// Answered identically to a fleet that does not exist — telling them apart
    /// would confirm a fleet id across a workspace boundary.
    NoSuchFleet,
    /// The fleet already holds [`crate::MAX_SCHEDULES_PER_FLEET`].
    TooMany,
    /// The fleet already holds [`crate::FLEET_SCHEDULES_MAX`] schedules it
    /// created itself.
    ///
    /// Its own refusal rather than [`Refused::TooMany`], because the remedy
    /// differs: the fleet removes one of its own, where a full fleet needs a
    /// person to remove one of theirs.
    FleetCapReached,
    /// This fleet already registered that upstream key.
    DuplicateKey,
}

impl Refused {
    /// The registry code this refusal answers with.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::NoSuchFleet => error_code::SCHEDULE_NOT_FOUND,
            Self::TooMany => error_code::SCHEDULE_LIMIT_REACHED,
            Self::FleetCapReached => error_code::SCHEDULE_CAP_REACHED,
            Self::DuplicateKey => error_code::SCHEDULE_KEY_TAKEN,
        }
    }

    /// The sentence the caller is told.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::NoSuchFleet => DETAIL_NOT_FOUND,
            Self::TooMany => DETAIL_TOO_MANY,
            Self::FleetCapReached => DETAIL_FLEET_CAP,
            Self::DuplicateKey => DETAIL_DUPLICATE,
        }
    }
}
