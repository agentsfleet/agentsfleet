//! Provider usage survives invalid semantic answers and model-version refusal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Measured provider token categories, without multiplying per-question totals.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionUsage {
    /// Complete input total, including cache reads and writes.
    pub input_tokens: u32,
    /// Completion tokens, including any provider-reported reasoning.
    pub output_tokens: u32,
    /// Input tokens served from cache, already included in `input_tokens`.
    pub cached_input_tokens: u32,
    /// Cache writes, already included in `input_tokens`.
    pub cache_write_tokens: u32,
}

/// Native probability answers after exact question/type/option validation.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionAnswer {
    /// Probability of yes; thresholds remain native policy.
    Noul {
        /// Probability from zero to one.
        noul: f64,
    },
    /// Categorical choice with its complete probability distribution.
    Choice {
        /// Selected option identity.
        choice: String,
        /// Provider-native confidence, absent when unavailable.
        confidence: Option<f64>,
        /// Probability of every allowed option.
        probabilities: BTreeMap<String, f64>,
    },
    /// Probability-weighted ordered rubric.
    Score {
        /// Expected zero-based level.
        score: f64,
        /// Provider-native confidence, absent when unavailable.
        confidence: Option<f64>,
        /// Distribution over zero-based levels.
        probabilities: BTreeMap<u32, f64>,
    },
}

/// Why a readable provider response cannot drive a native transition.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionInvalid {
    /// No model evidence, or a model outside the frozen accepted set.
    Model,
    /// Missing, unparseable or internally inconsistent token accounting.
    Usage,
    /// Answers are absent, duplicate, incomplete, mismatched or out of range.
    Answers,
}

/// A readable physical result, separately from whether it may advance work.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionOutcome {
    /// Complete typed evidence, still subject to native thresholds and authority.
    Answered {
        /// Exact reviewed question-key coverage.
        answers: BTreeMap<String, DecisionAnswer>,
    },
    /// The provider declined at least one required question.
    Refused,
    /// The response cannot authorize a semantic transition.
    Invalid {
        /// Bounded native diagnostic, without provider text.
        reason: DecisionInvalid,
    },
}

/// Usage is retained even when the semantic result is refused or stale.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionResult {
    /// Provider-reported name, without claiming an observable serving revision.
    pub reported_model: Option<String>,
    /// Measured usage, or an unknown reservation when unavailable.
    pub usage: Option<DecisionUsage>,
    /// Native interpretation of this physical response.
    pub result: DecisionOutcome,
}
