//! The sandboxes a runner holds frozen between a fleet's events, as a beat and
//! a poll list them, with the bounds that list is held to.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Most fleets one runner may hold a sandbox for: one per worker, and a runner
/// has at most `afd_core::limits::MAX_WORKERS`, which `afd_runner`'s suite
/// pins this to, since this crate carries no `afd_core`.
pub const HOLDS_MAX: usize = 64;

/// A fleet identifier's length as text, `afd_core::id::TEXT_LEN`, pinned the
/// same way.
pub const FLEET_ID_TEXT_BYTES: usize = 36;

/// The fleets a runner holds a frozen sandbox for, each by its identifier.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(transparent)]
pub struct HeldFleets<'a>(
    #[serde(borrow)]
    #[garde(
        length(max = HOLDS_MAX),
        inner(length(bytes, equal = FLEET_ID_TEXT_BYTES))
    )]
    pub Vec<Cow<'a, str>>,
);
