//! Non-secret inference settings persisted with a model and pinned on its run.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Largest provider-native model identity, in bytes: what one btree index row
/// holds.
///
/// The tighter of its two indexes is `model.tenant_models`' UNIQUE
/// (`tenant_id`, `workspace_id`, `model_name`, `secret_ref`). Postgres refuses
/// an index row past 2,704 bytes; less its 8-byte tuple header, the two 16-byte
/// `uuid`s, the name's 4-byte varlena header and a `secret_ref` at the vault's
/// 64-byte name bound with its 1-byte header, 2,595 remain. The catalogue's
/// UNIQUE (`provider`, `model_name`) holds more.
pub const MODEL_ID_MAX_BYTES: usize = 2704 - 8 - 16 - 16 - 4 - 65;

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
    /// Google's native `GenerateContent` API.
    Gemini,
    /// Ollama's native chat API.
    Ollama,
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
    #[garde(length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS))]
    pub base_url: String,
    /// Registered rig dialect, or absent for a plain compatible gateway. The
    /// native Gemini and Ollama APIs take none.
    #[garde(inner(length(chars, min = 1, max = af_validate::TEXT_MAX_CHARS)))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
    /// Model's configured maximum completion length, including reasoning.
    #[garde(range(min = 1))]
    pub max_output_tokens: u32,
}
