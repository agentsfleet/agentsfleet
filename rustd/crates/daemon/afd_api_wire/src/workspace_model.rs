//! A workspace's priced model choices: `GET /v1/workspaces/{workspace_id}/models`.

use serde::Serialize;

pub use crate::page::CursorPage;

/// What a model's saved settings support.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Eligibility {
    /// Runtime provider resolution is supported.
    pub runtime: bool,
}

/// Integer prices never cross a floating-point client boundary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = ModelPricing))]
#[derive(Debug, Serialize)]
pub struct Pricing {
    /// Input nanos per million tokens.
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub input_nanos_per_million: String,
    /// Output nanos per million tokens.
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub output_nanos_per_million: String,
    /// Runtime nanos per second.
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub runtime_nanos_per_second: String,
}

/// A priced explicit selection owned by the workspace's billing account.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct WorkspaceModel {
    /// Model selection reference, including an explicit shared platform default. Use it
    /// unchanged as a fleet's `model_ref`.
    #[serde(rename = "ref")]
    pub reference: String,
    /// Display label.
    pub label: String,
    /// Approved provider identifier.
    pub provider: String,
    /// Provider model identifier.
    pub model: String,
    /// Catalogue context bound.
    pub context_window_tokens: i32,
    /// Maximum output tokens; omitted when model settings are not configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    /// Supported purposes.
    pub eligibility: Eligibility,
    /// Exact current rates.
    pub pricing: Pricing,
}
