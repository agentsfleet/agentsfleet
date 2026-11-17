//! Read-only settled usage reports, separate from the wallet and raw ledger.

#[cfg(feature = "openapi")]
pub mod query;
mod usage;
pub use usage::Totals;

use serde::Serialize;
use std::borrow::Cow;

/// One spending group across the entire selected period.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingBreakdown))]
pub struct Breakdown<'a> {
    /// Fleet or model identifier; null identifies private setup usage.
    pub key: Option<Cow<'a, str>>,
    /// Recorded fleet label, when available.
    pub label: Option<Cow<'a, str>>,
    /// Exact filtered totals for this group.
    pub totals: Totals,
}

/// A fleet's complete recorded spending and model attribution.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingFleetBreakdown))]
pub struct FleetBreakdown<'a> {
    /// Fleet identity and exact filtered totals.
    #[serde(flatten)]
    pub group: Breakdown<'a>,
    /// Sorted distinct nonempty model identifiers recorded in matching lines.
    #[cfg_attr(feature = "openapi", schema(extensions(("x-stability" = json!("stable")))))]
    pub models: Vec<Cow<'a, str>>,
}

/// Settled totals for one UTC ledger creation date, including empty days.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingDaily))]
pub struct Daily {
    /// UTC calendar date in YYYY-MM-DD form.
    #[cfg_attr(feature = "openapi", schema(format = Date, extensions(("x-stability" = json!("stable")))))]
    pub day: String,
    /// Additive daily quantities; runs are counted on the period and fleet totals only.
    #[cfg_attr(feature = "openapi", schema(extensions(("x-stability" = json!("stable")))))]
    pub totals: Totals,
}

/// A fleet choice independent of the currently selected filters.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingFleetChoice))]
pub struct FleetChoice<'a> {
    /// Runtime fleet identifier; null identifies setup chat.
    pub id: Option<Cow<'a, str>>,
    /// Recorded fleet name, when available.
    pub name: Option<Cow<'a, str>>,
}

/// Available choices across the entire unfiltered period.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingChoices))]
pub struct Choices<'a> {
    /// Runtime fleets and private setup, without private draft labels.
    pub fleets: Vec<FleetChoice<'a>>,
}

/// Full-period settled spending.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingSpending))]
pub struct Spending<'a> {
    /// Grouped runtime fleets and private setup.
    pub by_fleet: Vec<FleetBreakdown<'a>>,
    /// Grouped model identifiers.
    pub by_model: Vec<Breakdown<'a>>,
}

/// The selected account's current settled usage report.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingReport))]
pub struct Report<'a> {
    /// `month`, `week` or `today`, interpreted in Coordinated Universal Time (UTC).
    pub period: Cow<'a, str>,
    /// Inclusive beginning of the selected period.
    #[serde(with = "af_wire::instant")]
    pub from_at: jiff::Timestamp,
    /// Inclusive end of the period; the report covers every whole UTC day through it.
    #[serde(with = "af_wire::instant")]
    pub through_at: jiff::Timestamp,
    /// Totals across every matching settled line.
    pub totals: Totals,
    /// Every UTC date in the selected interval, ascending.
    #[cfg_attr(feature = "openapi", schema(extensions(("x-stability" = json!("stable")))))]
    pub daily: Vec<Daily>,
    /// Full-period spending groups.
    pub spending: Spending<'a>,
    /// Unfiltered period choices.
    pub filters: Choices<'a>,
}
