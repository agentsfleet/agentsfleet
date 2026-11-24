//! Whose work a run runs.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Whose work a run runs: always a fleet, the one `event.fleet_id` names.
///
/// It is a tagged enum of one variant rather than a bare id. A frame naming any
/// other owner then fails to decode instead of running as a fleet's.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = RunOwnerRef))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Owner<'a> {
    /// A fleet's turn.
    Fleet {
        /// The fleet.
        #[serde(borrow)]
        fleet_id: Cow<'a, str>,
    },
}
