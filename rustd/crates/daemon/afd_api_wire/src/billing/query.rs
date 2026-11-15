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

/// Reports can select runtime or setup usage, or keep both.
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportKind {
    /// runtime or setup; absent selects both.
    pub activity_type: Option<String>,
}
