//! What a provider call measures, and the one port it reports through.
//!
//! The harness declares its measurements and the embedder decides where they
//! go. `agentsfleet-runner` installs its telemetry here (`afr_telemetry`); the
//! daemon installs nothing, so every call there is a load of an empty slot and
//! a return. Recording is a free function for the reason `afr_telemetry`'s
//! `record` gives: a measurement taken deep in a retry loop should not put a
//! telemetry parameter on every type between `main` and that loop.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use af_wire::usage::ObservedTokens;
use tracing::Span;

pub mod labels;
#[cfg(test)]
mod tests;

use self::labels::{
    CallOutcome, DecisionOutcome, DecisionProvider, DecisionPurpose, Provider, RetryReason,
};

/// Where a provider call's measurements go.
///
/// One method per measurement, each taking only its closed label set, so a
/// call site cannot attribute a measurement to a label the census never
/// declared.
pub trait Observer: Send + Sync + core::fmt::Debug {
    /// One native decision evaluation ended, including a refused preflight.
    fn decision(
        &self,
        provider: DecisionProvider,
        purpose: DecisionPurpose,
        outcome: DecisionOutcome,
        elapsed: Duration,
    );
    /// One provider stream request ended, with its first text when one came.
    fn model_call(
        &self,
        model: &str,
        provider: Provider,
        outcome: CallOutcome,
        elapsed: Duration,
        first_text: Option<Duration>,
    );
    /// The usage the provider reported for one request, partial usage included.
    fn model_tokens(&self, model: &str, provider: Provider, usage: ObservedTokens);
    /// A turn's send is about to be retried.
    fn retry(&self, provider: Provider, reason: RetryReason);
}

/// The observer the embedder installed, once per process.
static INSTALLED: OnceLock<Arc<dyn Observer>> = OnceLock::new();

/// Installs `observer` for the rest of the process.
///
/// Answers whether it took: the first install wins, because one process
/// exports through one pipeline.
pub fn install(observer: Arc<dyn Observer>) -> bool {
    INSTALLED.set(observer).is_ok()
}

/// Runs `observe` against the observer in force here, if there is one.
fn with(observe: impl FnOnce(&dyn Observer)) {
    #[cfg(any(test, feature = "test-util"))]
    if let Some(scoped) = crate::testing::observed::scoped_observer() {
        observe(scoped.as_ref());
        return;
    }
    if let Some(observer) = INSTALLED.get() {
        observe(observer.as_ref());
    }
}

/// Records one decision evaluation.
pub fn decision(
    provider: DecisionProvider,
    purpose: DecisionPurpose,
    outcome: DecisionOutcome,
    elapsed: Duration,
) {
    with(|observer| observer.decision(provider, purpose, outcome, elapsed));
}

/// Records one provider stream request.
pub fn model_call(
    model: &str,
    provider: Provider,
    outcome: CallOutcome,
    elapsed: Duration,
    first_text: Option<Duration>,
) {
    with(|observer| observer.model_call(model, provider, outcome, elapsed, first_text));
}

/// Records the usage one request reported.
pub fn model_tokens(model: &str, provider: Provider, usage: ObservedTokens) {
    with(|observer| observer.model_tokens(model, provider, usage));
}

/// Records a retried send.
pub fn retry(provider: Provider, reason: RetryReason) {
    with(|observer| observer.retry(provider, reason));
}

/// The span one reserved decision call is traced in.
///
/// Native identities and a reviewed revision only: callers pass validated
/// identifiers, never prompt or provider-error text.
pub fn decision_span(
    call_id: &str,
    model_id: &str,
    profile_revision: &str,
    provider: DecisionProvider,
    purpose: DecisionPurpose,
) -> Span {
    use af_observability::semconv::{ATTR_OPERATION_NAME, ATTR_PROVIDER_NAME, RUNNER_SCOPE_NAME};
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
