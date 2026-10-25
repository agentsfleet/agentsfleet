//! Bounded host observations, including the build that produced each heartbeat.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{CONTROLLER_NAME_MAX_BYTES, REPORT_CONTROLLERS_MAX};

/// What this host can actually enforce.
//
// Probed at startup and refreshed per beat. Each field is one enforcement
// mechanism a degraded reason can name.
///
// The flags stay separate booleans rather than collapsing into a bitset or an
// enum: each names a distinct mechanism a degraded reason quotes back to an
// operator, and the shape is the peer's, not this crate's to choose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "wire shape fixed by the peer; each flag is a separately reported mechanism"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReport<'a> {
    /// Source digest compiled into the reporting runner; absent for an unbound build.
    #[serde(borrow, default)]
    #[garde(inner(length(bytes, min = 64, max = 64)))]
    pub build_id: Option<Cow<'a, str>>,
    /// Filesystem isolation is available.
    #[garde(skip)]
    pub landlock: bool,
    /// System-call filtering is available.
    #[garde(skip)]
    pub seccomp: bool,
    /// Controllers present in the delegated cgroup's subtree control.
    // Stored and re-read by the runner page, so bounded: past either cap the
    // whole report reads as "nothing reported this beat".
    #[serde(borrow)]
    #[garde(
        length(max = REPORT_CONTROLLERS_MAX),
        inner(length(bytes, min = 1, max = CONTROLLER_NAME_MAX_BYTES))
    )]
    pub cgroup_controllers: Vec<Cow<'a, str>>,
    /// The sandbox launcher is available.
    #[garde(skip)]
    pub bubblewrap: bool,
    /// Whether the operating system can restrict outgoing network access to allowed hosts.
    #[garde(skip)]
    pub egress_enforcement: bool,
}
