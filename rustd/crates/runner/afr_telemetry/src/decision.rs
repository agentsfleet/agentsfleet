//! Decision trace metadata excludes state, question text and credentials.

use af_observability::semconv::{ATTR_OPERATION_NAME, ATTR_PROVIDER_NAME, RUNNER_SCOPE_NAME};
use tracing::Span;

use crate::labels::decision::{DecisionProvider, DecisionPurpose};

/// Trace one reserved call using native identities and a reviewed revision.
/// Callers supply validated identifiers, never prompt or provider-error text.
pub fn span(
    call_id: &str,
    model_id: &str,
    profile_revision: &str,
    provider: DecisionProvider,
    purpose: DecisionPurpose,
) -> Span {
    let provider = provider.as_str();
    let purpose = purpose.as_str();
    tracing::info_span!(target: RUNNER_SCOPE_NAME, "decision",
        { ATTR_OPERATION_NAME } = "decision",
        { ATTR_PROVIDER_NAME } = provider,
        "agentsfleet.decision.call.id" = call_id,
        "agentsfleet.decision.model.id" = model_id,
        "agentsfleet.decision.profile_revision" = profile_revision,
        "agentsfleet.decision.purpose" = purpose,
    )
}
