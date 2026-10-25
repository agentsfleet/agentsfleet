//! Exact money and recorded quantities; no estimated historical cost split.

use serde::Serialize;
use std::borrow::Cow;

/// Recorded settled totals over a period, spending group or activity.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingTotals))]
pub struct Totals {
    /// Actual combined credit charge, as integer nanodollar text.
    #[serde(with = "afd_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = afd_wire::nanos::Decimal))]
    pub charged_nanos: i64,
    /// Recorded input tokens, including the cached subset.
    pub input_tokens: i64,
    /// Recorded output tokens.
    pub output_tokens: i64,
    /// Recorded runtime in milliseconds.
    pub runtime_ms: i64,
    /// Number of individually settled usage lines.
    pub line_count: i64,
    /// Number of billed work items.
    pub activity_count: i64,
    /// Tokens without a published platform price.
    pub unpriced_tokens: i64,
}

/// One billed fleet lease or private setup attempt.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingActivity))]
pub struct Activity<'a> {
    /// Opaque identifier used to read its individual charges.
    pub id: Cow<'a, str>,
    /// `runtime` or `setup`.
    pub activity_type: Cow<'a, str>,
    /// Workspace where the work occurred.
    pub workspace_id: Cow<'a, str>,
    /// Runtime fleet identifier, retained after deletion.
    pub fleet_id: Option<Cow<'a, str>>,
    /// Runtime name captured in the ledger; never private draft text.
    pub fleet_name: Option<Cow<'a, str>>,
    /// Latest included usage-line timestamp.
    #[serde(with = "afd_wire::instant")]
    pub recorded_at: jiff::Timestamp,
    /// Exact totals for the selected period and filters.
    pub totals: Totals,
    /// Model identifiers recorded for this activity.
    pub models: Vec<Cow<'a, str>>,
    /// `platform`, `self_managed`, or both when work used both.
    pub postures: Vec<Cow<'a, str>>,
}

/// One physically recorded charge in a billed activity.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingCharge))]
pub struct Charge<'a> {
    /// Opaque ledger row identifier.
    pub id: Cow<'a, str>,
    /// Position within the activity.
    pub sequence: i64,
    /// Exact model identifier.
    pub model: Cow<'a, str>,
    /// Whether the platform or customer supplied the provider key.
    pub posture: Cow<'a, str>,
    /// Actual combined credit charge, as integer nanodollar text.
    #[serde(with = "afd_wire::nanos")]
    #[cfg_attr(feature = "openapi", schema(value_type = afd_wire::nanos::Decimal))]
    pub charged_nanos: i64,
    /// Recorded input tokens.
    pub input_tokens: i64,
    /// Recorded output tokens.
    pub output_tokens: i64,
    /// Recorded runtime in milliseconds.
    pub runtime_ms: i64,
    /// Tokens without a platform price.
    pub unpriced_tokens: i64,
    /// When the model request or runtime piece began.
    #[serde(with = "afd_wire::instant")]
    pub requested_at: jiff::Timestamp,
    /// When the charge settled.
    #[serde(with = "afd_wire::instant")]
    pub settled_at: jiff::Timestamp,
}
