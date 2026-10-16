//! Strict polling input, independent of the two placement payloads.

use crate::runner::HeldFleets;
use garde::Validate;
use serde::{Deserialize, Serialize};

/// `POST /v1/runners/me/leases` request.
///
/// What a polling runner tells the daemon about itself: the fleets whose
/// sandboxes it holds, so a held fleet's next event reaches it first. An empty
/// list may be omitted. The accepted work kinds are always required.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest<'a> {
    /// Every fleet this runner holds a frozen sandbox for. Absent decodes as
    /// empty.
    #[serde(borrow, default)]
    #[garde(dive)]
    #[cfg_attr(
        feature = "openapi",
        schema(value_type = Vec<String>, max_items = 64, min_length = 36, max_length = 36)
    )]
    pub holds: HeldFleets<'a>,
    /// The nonempty set of work protocols implemented by this worker.
    #[garde(skip)]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<super::LeaseKind>, min_items = 1, max_items = 2))]
    pub accepted_kinds: super::AcceptedKinds,
}
