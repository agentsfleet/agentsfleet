//! Tenant-owned reporting reads; domain queries remain in `afd_billing/report`.

mod input;
mod view;

pub use self::input::DETAIL_MODEL;

/// Why the per-activity charges route answers `410 Gone`.
pub const DETAIL_CHARGES_REMOVED: &str = "Per-activity charges were removed; GET /v1/tenants/me/billing/reports/current returns settled totals";

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantBilling as _};
use axum::Json;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::header;
use std::sync::Arc;

const EVENT_READ: &str = "billing_report_failed";
const EVENT_TENANT: &str = "billing_report_tenant_unresolved";
const CACHE_CONTROL: &str = "private, no-store";

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/tenants/me/billing/reports/current",
    tag = afd_http::openapi::tag::BILLING, operation_id = "get_tenant_billing_report",
    summary = "Read settled usage report",
    description = "Returns settled totals, spending groups, daily spend and filter choices for every whole Coordinated Universal Time (UTC) day of the selected month, week or today. Runs are counted on the period totals and each fleet. Unsettled usage is excluded.",
    params(
        afd_api_wire::billing::query::Filters,
        afd_api_wire::billing::query::ReportKind,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::billing::Report<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn report<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let tenant = super::tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;
    let input = input::read(query.as_deref().unwrap_or_default(), services.now())?;
    let rows = services
        .billing()
        .report(&tenant, &input.request)
        .await
        .map_err(Refusal::at(EVENT_READ))?;
    Ok((
        [(header::CACHE_CONTROL, CACHE_CONTROL)],
        Json(view::report(&rows, &input)?),
    )
        .into_response())
}

/// `GET /v1/tenants/me/billing/activities/{activity_id}/charges`, removed.
///
/// Kept in the route table, behind the same guards, so a client still calling
/// it learns the removal is permanent (`410`, `UZ-REQ-003`) instead of meeting
/// what reads as a routing fault.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/tenants/me/billing/activities/{activity_id}/charges",
    tag = afd_http::openapi::tag::BILLING, operation_id = "get_tenant_billing_activity_charges",
    summary = "Removed: per-activity charges",
    description = "Per-activity charges were removed. This route always answers 410 Gone with UZ-REQ-003; read settled totals from GET /v1/tenants/me/billing/reports/current.",
    params(("activity_id" = String, Path, description = "Ignored; the route is removed")),
    responses(
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn removed() -> Refusal {
    Refusal::coded(
        af_core::error_code::ENDPOINT_REMOVED,
        DETAIL_CHARGES_REMOVED,
    )
}
