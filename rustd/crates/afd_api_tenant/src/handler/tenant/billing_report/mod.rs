//! Tenant-owned reporting reads; domain queries remain in `afd_billing/report`.

mod cursor;
mod input;
mod view;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantBilling as _};
use afd_billing::biller::WorkKey;
use afd_billing::sql::work;
use axum::Json;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::header;
use std::sync::Arc;

const EVENT_READ: &str = "billing_report_failed";
const EVENT_DETAILS: &str = "billing_activity_charges_failed";
const EVENT_TENANT: &str = "billing_report_tenant_unresolved";
const CACHE_CONTROL: &str = "private, no-store";

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/tenants/me/billing/reports/current",
    tag = afd_http::openapi::tag::BILLING, operation_id = "get_tenant_billing_report",
    summary = "Read settled usage report",
    description = "Returns complete-period settled totals, spending groups and paginated activities. Month and today use Coordinated Universal Time (UTC). Later or unknown settlements are excluded. Follow starting_after with the same filters.",
    params(
        afd_api_wire::billing::query::Filters,
        afd_api_wire::billing::query::ReportKind,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::billing::Report),
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
    let input = input::read(
        query.as_deref().unwrap_or_default(),
        &tenant,
        None,
        services.now(),
    )?;
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

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/tenants/me/billing/activities/{activity_id}/charges",
    tag = afd_http::openapi::tag::BILLING, operation_id = "get_tenant_billing_activity_charges",
    summary = "List an activity's settled charges",
    description = "Lists individual settled charges for one billed runtime lease or private setup attempt within the selected period and filters. Foreign or unknown work returns an empty list.",
    params(
        ("activity_id" = String, Path, description = "Opaque billed activity identifier"),
        ("activity_type" = String, Query, description = "runtime or setup, required"),
        afd_api_wire::billing::query::Filters,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_api_wire::billing::Page<afd_api_wire::billing::Charge>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn charges<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(activity): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let tenant = super::tenant_of(
        &services,
        identity.person(),
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;
    let input = input::read(
        query.as_deref().unwrap_or_default(),
        &tenant,
        Some(&activity),
        services.now(),
    )?;
    let id = input::identifier(&activity)?;
    let work = match input.request.filter.work_type {
        Some(work::FLEET_LEASE) => WorkKey::FleetLease(id),
        Some(work::AUTHORING_ATTEMPT) => WorkKey::AuthoringAttempt(id),
        _ => return Err(Refusal::malformed(input::DETAIL_QUERY)),
    };
    let after = match input.position {
        Some(cursor::Position::Charge { sequence }) => Some(sequence),
        _ => None,
    };
    let rows = services
        .billing()
        .activity_charges(&tenant, &work, &input.request, after)
        .await
        .map_err(Refusal::at(EVENT_DETAILS))?;
    Ok((
        [(header::CACHE_CONTROL, CACHE_CONTROL)],
        Json(view::charges(&rows, &input.scope)?),
    )
        .into_response())
}
