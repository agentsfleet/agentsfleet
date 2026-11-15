//! Tenant-owned reporting reads; domain queries remain in `afd_billing/report`.

mod input;
mod view;

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
