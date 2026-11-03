//! Non-secret decision selection frozen with one issued fleet lease.

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{DecisionProfile, GoalDecisionPolicy, IntentDecisionPolicy};

/// One reviewed catalogue revision, retained unchanged for this lease.
/// Provider keys are resolved separately for each reserved physical call.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionAssignment {
    /// Native immutable revision identity used to resolve rates and authority.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub revision_id: String,
    /// Protocol, endpoint, model and capabilities from that same revision.
    #[garde(dive)]
    pub profile: DecisionProfile,
    /// Routing acceptance thresholds reviewed with the profile.
    #[garde(dive)]
    pub intent: IntentDecisionPolicy,
    /// Goal acceptance thresholds reviewed with the profile.
    #[garde(dive)]
    pub goal: GoalDecisionPolicy,
}
