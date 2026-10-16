//! Non-secret owner choices shared by browser and terminal selection.

use serde::Serialize;

/// Eligibility for each supported purpose is independent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Eligibility {
    /// Bounded setup authoring is supported.
    pub authoring: bool,
    /// Runtime provider resolution is supported.
    pub runtime: bool,
}

/// Integer prices never cross a floating-point client boundary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = ModelPricing))]
#[derive(Debug, Serialize)]
pub struct Pricing {
    /// Input nanos per million tokens.
    pub input_nanos_per_million: String,
    /// Output nanos per million tokens.
    pub output_nanos_per_million: String,
    /// Runtime nanos per second.
    pub runtime_nanos_per_second: String,
}

/// A priced explicit selection owned by the workspace's billing account.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct WorkspaceModel {
    /// Opaque registry reference, including an explicit platform default.
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
    /// Deployment output ceiling.
    pub max_output_tokens: i64,
    /// Supported purposes.
    pub eligibility: Eligibility,
    /// Exact current rates.
    pub pricing: Pricing,
}
