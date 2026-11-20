//! What a setup turn shows its creator: why it failed, and what it used.
//!
//! A setup turn runs on the common run path (`crate::run::SetupWork`); these
//! are the two facts the draft's message page and stream carry about it.

use serde::{Deserialize, Serialize};

/// Allowed failure reasons, excluding user-supplied diagnostics and links.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    /// The workspace owner's balance cannot cover assignment or renewal.
    InsufficientBalance,
    /// The pinned authoring model is no longer eligible.
    ModelUnavailable,
    /// The assembled context cannot fit its hard bounds.
    ContextTooLarge,
    /// A known-usage provider failure permits a new message.
    ProviderFailed,
    /// A physical dispatch has an unresolved provider delivery.
    ProviderUsageUnknown,
    /// The attempt exhausted its wall-clock allowance.
    AttemptTimedOut,
    /// Effective setup policy changed and requires review.
    PolicyChanged,
    /// The run would not fit the runner protocol's frame, so it was failed
    /// at assignment and never sent.
    FrameTooLarge,
}

/// Whether all reported provider usage is known.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    /// All physical calls have authoritative usage.
    Known,
    /// At least one physical call has an unresolved delivery.
    Unknown,
}

/// Creator-visible usage; unknown provider figures remain absent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Context knowledge measured by the server, independent of runner estimates.
    pub knowledge: Knowledge,
    /// Cumulative input, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Cumulative output, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Physical provider cost, absent while unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_cost_nanos: Option<String>,
    /// Upward-rounded cumulative runtime cost.
    pub runtime_cost_nanos: String,
}
