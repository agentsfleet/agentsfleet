//! Workspace model selection exposes prices and eligibility, never key locations.

use serde::{Deserialize, Serialize};

/// Optional intended use of an explicitly selected owner model.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPurpose {
    /// Bounded private setup generation.
    Authoring,
    /// Shared fleet execution.
    Runtime,
}

/// Page size and purpose bind every opaque continuation.
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelsQuery {
    /// Rows scanned per page, 1..100.
    #[serde(default = "default_limit")]
    #[garde(range(min = 1, max = super::MAX_PAGE_SIZE))]
    pub limit: u32,
    /// Restrict to supported use when supplied.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(skip)]
    pub purpose: Option<ModelPurpose>,
    /// Opaque owner/workspace/person/purpose/page-size bound continuation.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = super::MAX_CURSOR_BYTES)))]
    pub starting_after: Option<String>,
}

fn default_limit() -> u32 {
    super::DEFAULT_PAGE_SIZE
}

/// Independent support facts, not permission receipts.
#[derive(Debug, Serialize)]
pub struct ModelEligibility {
    /// This deployment can perform bounded authoring with the selection.
    pub authoring: bool,
    /// Existing runtime provider resolution accepts the selection.
    pub runtime: bool,
}

/// Exact integer rates as decimal strings.
#[derive(Debug, Serialize)]
pub struct ModelPricing {
    /// Provider input nanos per million tokens.
    pub input_nanos_per_million: String,
    /// Provider output nanos per million tokens.
    pub output_nanos_per_million: String,
    /// Platform runtime nanos per second, independent of provider management.
    pub runtime_nanos_per_second: String,
}

/// A configured and priced choice owned by the workspace's billing account.
#[derive(Debug, Serialize)]
pub struct WorkspaceModel {
    /// Opaque explicit reference; it is not the provider's model identifier.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Human selection label.
    pub label: String,
    /// Approved provider identifier.
    pub provider: String,
    /// Provider model identifier.
    pub model: String,
    /// Catalogue input/output context bound.
    pub context_window_tokens: i32,
    /// Deployment's bounded output ceiling for this choice.
    pub max_output_tokens: i64,
    /// Supported uses, independently evaluated.
    pub eligibility: ModelEligibility,
    /// Current priced catalogue rates.
    pub pricing: ModelPricing,
}
