//! The root coordinator receives accounted evidence, never a dispatch credential.

use af_wire::task::{DecisionCallState, DecisionRecorded};

/// A native physical call and its non-secret, originally bound result.
/// Existing unresolved reservations remain unresolved; reading never sends again.
pub struct DecisionEvaluation {
    /// Native physical identity used by binding/report proof checks.
    pub call_id: String,
    /// Original-owner accounting state.
    pub state: DecisionCallState,
    /// Absent until a complete provider receipt is available.
    pub recorded: Option<Box<DecisionRecorded>>,
}

impl std::fmt::Debug for DecisionEvaluation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionEvaluation")
            .field("call_id", &self.call_id)
            .field("state", &self.state)
            .field("recorded", &self.recorded.is_some())
            .finish()
    }
}
