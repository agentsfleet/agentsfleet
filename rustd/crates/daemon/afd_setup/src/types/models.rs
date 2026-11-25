//! Workspace model selection exposes prices and eligibility, never key locations.

use serde::{Deserialize, Serialize};

/// Optional intended use of an explicitly selected owner model.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelPurpose {
    /// Shared fleet execution.
    Runtime,
}

/// Page size and purpose bind every opaque continuation.
#[derive(Debug, Deserialize, garde::Validate)]
#[garde(context(af_validate::Ceiling as ceiling))]
#[serde(deny_unknown_fields)]
pub struct ModelsQuery {
    /// Rows scanned per page, up to the deployment's list page (`[api]`);
    /// its default when absent.
    #[serde(default)]
    #[garde(inner(range(min = 1, max = ceiling.max())))]
    pub limit: Option<u32>,
    /// Restrict to supported use when supplied.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(skip)]
    pub purpose: Option<ModelPurpose>,
    /// Opaque owner/workspace/person/purpose/page-size bound continuation.
    #[serde(default, deserialize_with = "super::present")]
    #[garde(inner(length(bytes, min = 1, max = super::MAX_CURSOR_BYTES)))]
    pub starting_after: Option<String>,
}

/// Independent support facts, not permission receipts.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ModelEligibility {
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
    /// Saved output ceiling; absent until inference settings are configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    /// Supported uses, independently evaluated.
    pub eligibility: ModelEligibility,
    /// Current priced catalogue rates.
    pub pricing: ModelPricing,
}
