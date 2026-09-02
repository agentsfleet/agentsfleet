//! The query-parameter shapes more than one route reads.
//!
//! Same reason as [`super::path`]: a parameter's name is a literal token the
//! macro will not take a `const` for, so a filter read by three routes was
//! spelled three times. `IntoParams` makes the name a field identifier, which
//! is one declaration and a compile-time rename.
//!
//! Only the SHARED shapes live here. A filter one route reads stays spelled at
//! that route, where it can describe what it means there — a shape lifted for
//! its own sake would be indirection rather than discipline.

use utoipa::IntoParams;

/// A keyset page, as the paginated collections spell it.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct Page {
    /// Token from `next_cursor` in the previous response. Omit it for the first page.
    pub cursor: Option<String>,
    /// Number of records to return. The default is 50. Valid values are 1 through 200.
    pub limit: Option<String>,
}

/// A keyset page, as the tenant collections spell it.
///
/// `starting_after` rather than `cursor`, and the two are NOT interchangeable:
/// the tenant surface published this spelling and clients send it.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TenantPage {
    /// Cursor returned as `next_cursor` by the previous page.
    pub starting_after: Option<String>,
    /// How many rows to return.
    pub limit: Option<String>,
}

/// What an event history is narrowed by.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventFilter {
    /// Opaque base64url-no-pad cursor returned in `next_cursor`. Mutually exclusive with `since`.
    pub cursor: Option<String>,
    /// Glob filter against the row's actor. `*` is a wildcard.
    pub actor: Option<String>,
    /// Lower bound on `created_at`. Accepts Go-style durations (15s/30m/2h/7d) or RFC 3339 timestamps.
    pub since: Option<String>,
    /// How many rows to return.
    pub limit: Option<String>,
}

/// What a workspace list is filtered and paged by.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WorkspaceFilter {
    /// Exact tenant-scoped workspace name.
    pub name: Option<String>,
    /// Cursor returned as `next_cursor` by the previous page.
    pub starting_after: Option<String>,
    /// How many rows to return.
    pub limit: Option<String>,
}

/// What a tenant's model registry is filtered and paged by.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ModelEntryFilter {
    /// Exact match on the normalized provider. At most 128 bytes once
    /// normalized; empty after normalization is treated as absent.
    pub provider: Option<String>,
    /// Opaque cursor from a previous page's `next_cursor`, bound to the filters
    /// and page size that produced it. Only the sort boundary is read from it —
    /// the filters applied are always the request's.
    pub starting_after: Option<String>,
    /// Rows per page, 1..100. Defaults to 50.
    pub limit: Option<String>,
}

/// The one-time value a connector flow is resumed by.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConnectorState {
    /// Signed one-time value supplied through the connection URL.
    pub state: String,
}

/// What a provider hands back on the consent redirect.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ConnectorCallback {
    /// Authorization code supplied by the provider when required.
    pub code: Option<String>,
    /// Installation identifier supplied by providers such as GitHub.
    pub installation_id: Option<String>,
    /// Provider data-center location supplied by multi-region providers such as Zoho.
    pub location: Option<String>,
}
