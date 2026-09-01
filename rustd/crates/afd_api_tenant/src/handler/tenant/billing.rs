//! The two reads over a tenant's money: the wallet, and the charges ledger.
//!
//! The port of `tenant_billing.zig`, sentence for sentence: the limit
//! refusals, the empty-cursor-is-first-page rule, and the bare "Tenant
//! context required" are wire facts a dashboard mid-cutover may already be
//! matching on, so each is pinned here rather than shared with a family that
//! spells its own.

use std::borrow::Cow;
use std::sync::Arc;

use afd_billing::tenant::cursor;
use afd_billing::tenant::{CHARGES_LIMIT_DEFAULT, CHARGES_LIMIT_MAX, ChargeRow, Wallet};
use afd_wire::tenant::{BillingResponse, ChargeSummary, ChargesResponse};
use axum::Json;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantBilling as _};

use super::{parameter, tenant_of};

/// The scoped events each verb's failures are logged under.
const EVENT_SNAPSHOT: &str = "billing_snapshot_failed";
const EVENT_CHARGES: &str = "billing_charges_failed";
const EVENT_TENANT: &str = "billing_tenant_unresolved";

/// The refusal a `limit` that is not a number earns.
///
/// `pub`, like its sibling below: the router suite asserts these sentences by
/// identity rather than by a respelling that could drift (RULE UFS).
pub const DETAIL_LIMIT_NOT_NUMERIC: &str = "limit must be a positive integer";

/// The refusal a `limit` outside `1..=200` earns.
pub const DETAIL_LIMIT_RANGE: &str = "limit must be between 1 and 200";

/// `GET /v1/tenants/me/billing` — the wallet snapshot.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/billing",
    tag = afd_http::openapi::tag::BILLING,
    operation_id = "get_tenant_billing",
    summary = "Read the tenant balance",
    description = concat!(
        "Returns the balance shared by every workspace in the tenant. The ",
        "response also shows whether the balance is empty. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = BillingResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn snapshot<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let tenant = tenant_of(
        &services,
        person,
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let wallet = services
        .billing()
        .snapshot(&tenant)
        .await
        .map_err(Refusal::at(EVENT_SNAPSHOT))?;
    Ok(Json(wallet_response(&wallet)).into_response())
}

/// `GET /v1/tenants/me/billing/charges` — one page of the ledger.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/billing/charges",
    tag = afd_http::openapi::tag::BILLING,
    operation_id = "get_tenant_billing_charges",
    summary = "List tenant charges",
    description = concat!(
        "Returns charge records with the newest record first. Use `limit` and ",
        "`cursor` to read more records. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ChargesResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn charges<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let query = query.unwrap_or_default();
    let limit = parse_limit(parameter(&query, "limit"))?;
    // `?cursor=` with an empty value is the first page expressed verbosely,
    // not a malformed token — the Zig handler's rule, kept to the byte.
    let boundary = match parameter(&query, "cursor").filter(|token| !token.is_empty()) {
        None => None,
        Some(token) => Some(cursor::parse(token).map_err(Refusal::at(EVENT_CHARGES))?),
    };

    let tenant = tenant_of(
        &services,
        person,
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let rows = services
        .billing()
        .charges(&tenant, limit, boundary.as_ref())
        .await
        .map_err(Refusal::at(EVENT_CHARGES))?;
    Ok(Json(page_response(&rows, limit)).into_response())
}

/// The wallet as the wire shows it.
///
/// `is_exhausted` is derived HERE, not stored: the row holds one fact — when
/// the balance reached zero — and the boolean is that fact restated, exactly
/// as `tenant_billing.zig` computes it at the response site.
const fn wallet_response(wallet: &Wallet) -> BillingResponse {
    BillingResponse {
        balance_nanos: wallet.balance_nanos,
        updated_at: wallet.updated_at_ms,
        is_exhausted: wallet.exhausted_at_ms.is_some(),
        exhausted_at: wallet.exhausted_at_ms,
    }
}

/// One page, and the cursor that continues it.
///
/// A cursor is emitted only when the page is FULL, like the api-key list's:
/// a short page means the walk is done, and a token pointing past it would
/// come back as an empty page the client asked for.
fn page_response(rows: &[ChargeRow], limit: u32) -> ChargesResponse<'_> {
    // `try_from` rather than a cast: a page longer than `u32` cannot happen —
    // the limit is bounded at two hundred — and a cast would say so by
    // silently truncating rather than by being unreachable.
    let full = u32::try_from(rows.len()).is_ok_and(|count| count == limit);
    let next_cursor = full
        .then(|| rows.last())
        .flatten()
        .map(|last| Cow::Owned(cursor::render(last.recorded_at, &last.id)));
    ChargesResponse {
        items: rows.iter().map(summary).collect(),
        next_cursor,
    }
}

/// One row as the wire shows it.
fn summary(row: &ChargeRow) -> ChargeSummary<'_> {
    ChargeSummary {
        id: Cow::Borrowed(&row.id),
        tenant_id: Cow::Borrowed(&row.tenant_id),
        workspace_id: row.workspace_id.as_deref().map(Cow::Borrowed),
        fleet_id: row.fleet_id.as_deref().map(Cow::Borrowed),
        event_id: Cow::Borrowed(&row.event_id),
        charge_type: Cow::Borrowed(&row.charge_type),
        posture: Cow::Borrowed(&row.posture),
        model: Cow::Borrowed(&row.model),
        credit_deducted_nanos: row.credit_deducted_nanos,
        token_count_input: row.token_count_input,
        token_count_output: row.token_count_output,
        wall_ms: row.wall_ms,
        recorded_at: row.recorded_at,
    }
}

/// The page size the caller asked for, or the refusal their spelling earns.
///
/// The port of `parseLimit`: absent means the default, a non-number is one
/// sentence, and zero or past the cap is the other. `u32::from_str` refuses a
/// sign the way Zig's unsigned `parseInt` does, so `-1` lands on the
/// not-numeric sentence on both daemons.
fn parse_limit(raw: Option<&str>) -> Result<u32, Refusal> {
    let Some(raw) = raw else {
        return Ok(CHARGES_LIMIT_DEFAULT);
    };
    let limit: u32 = raw
        .parse()
        .map_err(|_not_numeric| Refusal::malformed(DETAIL_LIMIT_NOT_NUMERIC))?;
    if limit == 0 || limit > CHARGES_LIMIT_MAX {
        return Err(Refusal::malformed(DETAIL_LIMIT_RANGE));
    }
    Ok(limit)
}

#[cfg(test)]
mod tests {
    use super::{CHARGES_LIMIT_DEFAULT, parse_limit};

    // `.ok()` throughout, because a `Refusal` is a rendered response and
    // deliberately not `Debug` — which side of the `Result` a spelling lands
    // on is the whole assertion anyway.

    #[test]
    fn a_missing_limit_is_the_default_page_size() {
        assert_eq!(parse_limit(None).ok(), Some(CHARGES_LIMIT_DEFAULT));
    }

    #[test]
    fn the_boundaries_are_part_of_the_range() {
        // 1 and 200 pass; 0 and 201 do not. The edges are asserted because
        // they are exactly where an off-by-one between the two daemons would
        // live.
        assert_eq!(parse_limit(Some("1")).ok(), Some(1));
        assert_eq!(parse_limit(Some("200")).ok(), Some(200));
        assert!(parse_limit(Some("0")).is_err(), "zero rows is not a page");
        assert!(parse_limit(Some("201")).is_err(), "past the cap");
    }

    #[test]
    fn everything_that_is_not_a_count_is_refused() {
        for wrong in ["lots", "-1", "1.5", ""] {
            assert!(
                parse_limit(Some(wrong)).is_err(),
                "{wrong:?} is not a page size"
            );
        }
    }
}
