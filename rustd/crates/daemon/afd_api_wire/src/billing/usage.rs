//! Exact money and recorded quantities; no estimated historical cost split.

use serde::Serialize;

/// Recorded settled totals over a period, a spending group or a day.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingTotals))]
pub struct Totals {
    /// Actual combined credit charge, as integer nanodollar text.
    #[serde(with = "af_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = af_wire::nanos::Decimal))]
    pub charged_nanos: i64,
    /// Recorded input tokens, including the cached subset.
    pub input_tokens: i64,
    /// Recorded output tokens.
    pub output_tokens: i64,
    /// Recorded runtime in milliseconds.
    pub runtime_ms: i64,
    /// Number of individually settled usage lines.
    pub line_count: i64,
    /// Runs counted on their first runtime slice's day. Present on the period
    /// totals and each fleet group; absent on model and daily groups, since one
    /// run spans several models and days.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(extensions(("x-stability" = json!("stable")))))]
    pub activity_count: Option<i64>,
    /// Tokens without a published platform price.
    pub unpriced_tokens: i64,
}
