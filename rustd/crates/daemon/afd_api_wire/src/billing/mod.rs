//! Read-only settled usage reports, separate from the wallet and raw ledger.

#[cfg(feature = "openapi")]
pub mod query;
mod usage;
pub use usage::{Activity, Charge, Totals};

use serde::Serialize;
use std::borrow::Cow;

/// A bounded list with the standard envelope.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingPage))]
pub struct Page<'a, T> {
    /// Items on this page.
    pub items: Vec<T>,
    /// Full matching count, or null when it was not computed.
    pub total: Option<i64>,
    /// Continue with `starting_after`, or null on the final page.
    pub next_cursor: Option<Cow<'a, str>>,
}

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
    /// Exact model identifiers recorded during the period.
    pub models: Vec<Cow<'a, str>>,
}

/// Full-period settled spending, independent of activity pagination.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingSpending))]
pub struct Spending<'a> {
    /// Grouped runtime fleets and private setup.
    pub by_fleet: Vec<Breakdown<'a>>,
    /// Grouped model identifiers.
    pub by_model: Vec<Breakdown<'a>>,
}

/// The selected account's current settled usage report.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BillingReport))]
pub struct Report<'a> {
    /// `month` or `today`, both interpreted in Coordinated Universal Time (UTC).
    pub period: Cow<'a, str>,
    /// Inclusive beginning of the selected period.
    #[serde(with = "afd_wire::instant")]
    pub from_at: jiff::Timestamp,
    /// Inclusive reporting cutoff, retained by continuations.
    #[serde(with = "afd_wire::instant")]
    pub through_at: jiff::Timestamp,
    /// Totals across every matching settled line.
    pub totals: Totals,
    /// Full-period spending groups.
    pub spending: Spending<'a>,
    /// Unfiltered period choices, never limited to the current page.
    pub filters: Choices<'a>,
    /// Bounded grouped activities, newest first.
    pub activities: Page<'a, Activity<'a>>,
}
