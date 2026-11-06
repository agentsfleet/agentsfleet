//! Bounded typed decisions; native authority and transitions remain with callers.

mod answer;
mod assignment;
mod config;
mod question;
mod result;

use garde::Validate;
use serde::{Deserialize, Serialize};

pub use afd_validate::serialized_within;
pub use answer::answer_matches;
pub use assignment::DecisionAssignment;
pub use config::{
    DecisionConfiguration, DecisionModelSettings, DecisionThreshold, GoalDecisionPolicy,
    IntentDecisionPolicy,
};
pub(crate) use question::state_bound;
pub use question::{ChoiceOption, DecisionQuestion, DecisionRequest, NamedQuestion, NoulCriteria};
pub use result::{DecisionAnswer, DecisionInvalid, DecisionOutcome, DecisionResult, DecisionUsage};

/// Maximum serialized evidence supplied to one decision.
pub const STATE_MAX_BYTES: usize = 16 * 1024;
/// Maximum typed questions in one physical request.
pub const QUESTIONS_MAX: usize = 8;
/// Maximum compiled question, including all native candidate options.
/// Authored questions retain their separate 1 KiB setup bound.
pub const QUESTION_MAX_BYTES: usize = 16 * 1024;
/// Maximum response collected from the provider.
pub const REPLY_MAX_BYTES: usize = 64 * 1024;

/// Native decision protocols; none is a structured-chat substitute.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionApi {
    /// `TypeSafe` System One.
    Typesafe,
    /// `OpenAI` Decisions.
    Openai,
    /// Perplexity Decisions.
    Perplexity,
    /// Cloudflare Workers AI Clef envelope.
    Cloudflare,
}

/// Why the native runtime reserved a physical decision call.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionPurpose {
    /// Interpret newest input against trusted fleet/task summaries.
    Intent,
    /// Assess the accepted objective against current evidence.
    Goal,
}

impl DecisionPurpose {
    /// Canonical purpose used by native accounting and protocol records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Goal => "goal",
        }
    }
}

/// Capabilities reviewed with a catalogue profile; unsupported work is refused.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionCapabilities {
    /// True/false probability.
    pub noul: bool,
    /// Selected option and full distribution.
    pub choice: bool,
    /// Ordered rubric and full distribution.
    pub score: bool,
}

impl DecisionCapabilities {
    /// Whether this reviewed capability set represents the complete question type.
    #[must_use]
    pub const fn supports(self, question: &DecisionQuestion) -> bool {
        match question {
            DecisionQuestion::Noul { .. } => self.noul,
            DecisionQuestion::Choice { .. } => self.choice,
            DecisionQuestion::Score { .. } => self.score,
        }
    }
}

/// Non-secret catalogue metadata frozen for a lease's selected decision model.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct DecisionProfile {
    /// Catalogue model identity; this is not another model registry.
    #[garde(length(bytes, min = 36, max = 36), pattern(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$"))]
    pub model_id: String,
    /// Reviewed profile revision used for acceptance and accounting.
    #[garde(length(bytes, min = 1, max = 128), pattern(r"^[A-Za-z0-9_.:-]+$"))]
    pub revision: String,
    /// Native API selected by its catalogue entry.
    #[garde(skip)]
    pub api: DecisionApi,
    /// Exact reviewed HTTPS endpoint, including the native API path.
    #[garde(length(bytes, min = 1, max = 2048))]
    pub endpoint: String,
    /// Requested model; an alias is not evidence of a resolved serving version.
    #[cfg_attr(feature = "openapi", schema(min_length = 1, max_length = 256))]
    #[garde(length(bytes, min = 1, max = crate::inference::MODEL_ID_MAX_BYTES))]
    pub model: String,
    /// Reviewed reported model names; activation owns alias/version evidence.
    #[garde(length(min = 1, max = 8), inner(length(bytes, min = 1, max = 128)))]
    pub accepted_models: Vec<String>,
    /// Capabilities verified for this exact profile revision.
    #[garde(skip)]
    pub capabilities: DecisionCapabilities,
    /// Conservative serialized evidence/question byte ceiling established for
    /// this profile, preventing providers from silently truncating its input.
    #[garde(range(min = 1, max = 24576))]
    pub input_max_bytes: u32,
}
