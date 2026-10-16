//! Non-secret inference settings persisted with a model and pinned on its lease.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The provider API rig uses for this configured model.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceApi {
    /// OpenAI-compatible chat completions.
    Chat,
    /// `OpenAI` Responses.
    Responses,
    /// Anthropic Messages.
    Messages,
}

impl InferenceApi {
    /// Messages and Responses accept images in tool results.
    #[must_use]
    pub const fn carries_images(self) -> bool {
        matches!(self, Self::Messages | Self::Responses)
    }
}

/// Admin-owned model settings; credentials remain separate vault references.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelInference {
    /// Explicit provider API; never inferred from a model identifier.
    #[garde(skip)]
    pub api: InferenceApi,
    /// HTTPS provider base. Host safety is proven by the credential service.
    #[garde(length(bytes, min = 1, max = 2048))]
    pub base_url: String,
    /// Registered rig dialect, or absent for a plain compatible gateway.
    #[garde(inner(length(bytes, min = 1, max = 64)))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Model's configured maximum completion length, including reasoning.
    #[garde(range(min = 1))]
    pub max_output_tokens: u32,
}
