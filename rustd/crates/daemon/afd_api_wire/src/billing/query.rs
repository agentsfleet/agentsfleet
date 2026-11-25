//! Shared `OpenAPI` query shapes; handlers validate the original raw query.

/// Filters for the settled usage report.
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct Filters {
    /// month (default), week (seven calendar days) or today, in Coordinated Universal Time (UTC).
    pub period: Option<String>,
    /// Exact runtime fleet identifier.
    pub fleet_id: Option<String>,
    /// Inclusive cutoff; defaults to now. The report covers every whole UTC day through it.
    #[param(format = DateTime)]
    pub through_at: Option<String>,
}

/// Reports can select one kind of work; every line today is a fleet run's.
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportKind {
    /// `runtime`, a fleet run's usage; absent selects every line.
    pub activity_type: Option<String>,
}
