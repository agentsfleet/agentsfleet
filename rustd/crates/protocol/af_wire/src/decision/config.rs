//! Reviewed decision configuration stored beside an existing catalogue identity.

use garde::Validate;
use serde::{Deserialize, Serialize};

use super::{DecisionApi, DecisionCapabilities};

/// Probability and native confidence requirements, in integer basis points:
/// 10,000 is one, so no requirement exceeds it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionThreshold {
    /// The selected option's required probability; 10,000 means one.
    #[cfg_attr(feature = "openapi", schema(maximum = 10000))]
    #[garde(range(max = 10000))]
    pub probability_bps: u16,
    /// Required provider-native confidence; absence cannot meet this bound.
    #[cfg_attr(feature = "openapi", schema(maximum = 10000))]
    #[garde(range(max = 10000))]
    pub confidence_bps: u16,
}

/// Reviewed goal-template revision and acceptance thresholds.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct GoalDecisionPolicy {
    /// Exact native question revision implemented by this runner generation.
    #[cfg_attr(feature = "openapi", schema(pattern = "^dt-goal-v1$"))]
    #[garde(custom(af_validate::exactly("dt-goal-v1")))]
    pub questions_revision: String,
    /// The task is done when every completion statement meets this Noul
    /// threshold, whatever the next action says; independently checked native
    /// evidence still applies.
    #[cfg_attr(feature = "openapi", schema(maximum = 10000))]
    #[garde(range(max = 10000))]
    pub statement_min_bps: u16,
    /// Continue acceptance, subject to native remaining limits.
    #[garde(dive)]
    pub continue_action: DecisionThreshold,
    /// Wait acceptance, subject to a concrete native dependency.
    #[garde(dive)]
    pub wait: DecisionThreshold,
}

/// Editable, non-secret model settings; catalogue identity is supplied natively.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionModelSettings {
    /// Immutable reviewed definition revision; changing settings needs a new one.
    // utoipa reads only a literal: 2676 is `REVISION_MAX_BYTES`, each
    // 1_048_576 below is `TEXT_MAX_CHARS`, and `tests/durable_task_validation.rs`
    // pins both against the published schema.
    #[cfg_attr(
        feature = "openapi",
        schema(min_length = 1, max_length = 2676, pattern = "^[A-Za-z0-9_.:-]+$")
    )]
    #[garde(
        length(bytes, min = 1, max = super::REVISION_MAX_BYTES),
        pattern(r"^[A-Za-z0-9_.:-]+$")
    )]
    pub revision: String,
    /// Native decision API; never inferred from a provider or model name.
    #[garde(skip)]
    pub api: DecisionApi,
    /// Exact reviewed HTTPS endpoint, including its API path.
    #[cfg_attr(feature = "openapi", schema(min_length = 1, max_length = 1_048_576))]
    #[garde(
        length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS),
        custom(af_validate::nul_free)
    )]
    pub endpoint: String,
    /// Provider-reported names supported by this revision's evaluation evidence.
    #[cfg_attr(
        feature = "openapi",
        schema(min_items = 1, min_length = 1, max_length = 1_048_576)
    )]
    #[garde(
        length(min = 1),
        inner(
            length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS),
            custom(af_validate::nul_free)
        )
    )]
    pub accepted_models: Vec<String>,
    /// Question types verified against the provider API for this definition.
    #[garde(skip)]
    pub capabilities: DecisionCapabilities,
    /// Complete encoded-request ceiling established for the provider.
    #[cfg_attr(feature = "openapi", schema(minimum = 1))]
    #[garde(range(min = 1))]
    pub input_max_bytes: u32,
    /// Goal question and threshold policy.
    #[garde(dive)]
    pub goal: GoalDecisionPolicy,
    /// Digest of the frozen evaluation inputs and graded outcomes.
    #[cfg_attr(feature = "openapi", schema(pattern = "^[0-9a-f]{64}$"))]
    #[garde(pattern(r"^[0-9a-f]{64}$"))]
    pub evaluation_digest: String,
    /// Evidence location reviewed before activation; it carries no credentials.
    #[cfg_attr(feature = "openapi", schema(min_length = 1, max_length = 1_048_576))]
    #[garde(
        length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS),
        custom(af_validate::nul_free)
    )]
    pub evaluation_source_url: String,
    /// Recorded evaluation instant; actual activation proof remains an operator duty.
    #[serde(with = "crate::instant")]
    #[garde(skip)]
    pub evaluated_at: jiff::Timestamp,
}

/// Activation can change without mutating an immutable reviewed definition.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionConfiguration {
    /// False fences new dispatch while retaining historical accounting.
    #[garde(skip)]
    pub enabled: bool,
    /// Immutable reviewed definition, separate from the lifecycle switch.
    #[garde(dive)]
    pub profile: DecisionModelSettings,
}
