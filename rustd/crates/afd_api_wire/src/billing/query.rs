//! Shared `OpenAPI` query shapes; handlers validate the original raw query.

/// Filters and paging shared by settled reports and activity details.
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct Filters {
    /// month (default) or today, in Coordinated Universal Time (UTC).
    pub period: Option<String>,
    /// Exact runtime fleet identifier.
    pub fleet_id: Option<String>,
    /// Exact recorded model identifier.
    pub model: Option<String>,
    /// Inclusive cutoff with millisecond precision; defaults to now. Use the report timestamp to preserve its view.
    #[param(format = DateTime)]
    pub through_at: Option<String>,
    /// Page size: 1 to 100, default 50.
    pub limit: Option<u32>,
    /// Opaque continuation for the same query and page size.
    pub starting_after: Option<String>,
}

/// Reports can select either activity kind or retain both.
#[derive(Debug, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportKind {
    /// runtime or setup; absent selects both.
    pub activity_type: Option<String>,
}
