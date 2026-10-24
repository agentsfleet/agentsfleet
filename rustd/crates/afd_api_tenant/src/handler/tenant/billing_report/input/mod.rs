//! Bound first, parse second; malformed filters never become an unfiltered report.

use afd_api_wire::admin::MODEL_ID_MAX_BYTES;
use afd_billing::report::{
    BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY,
    Boundary, Filter, Request, Window,
};
use afd_billing::sql::work;
use afd_core::id::{TEXT_LEN, Uuid7};
use afd_core::paging::{CEILING, QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_validate::{Limit, Sentences, nul_free};
use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::cursor::{Cursor, Position, Scope};
use crate::handler::{Refusal, decoded_parameter};

pub(super) const DETAIL_QUERY: &str = "Invalid billing report filters";
pub(super) const DETAIL_CURSOR: &str = "Invalid billing report continuation";
const DETAIL_PERIOD: &str = "period must be month or today";
const DETAIL_LIMIT: &str = "limit must be between 1 and 100";
const DETAIL_MODEL: &str = "Invalid billing model filter";
const QUERY_PERIOD: &str = "period";
const QUERY_MODEL: &str = "model";
const CURSOR_MAX_BYTES: usize = 2048;
const INSTANT_MAX_BYTES: usize = 64;
const PERIOD_MAX_BYTES: usize = 5;
const ACTIVITY_TYPE_MAX_BYTES: usize = 7;
const LIMIT_MAX_BYTES: usize = 20;
const SENTENCES: Sentences = Sentences::new(
    &[
        (QUERY_PERIOD, DETAIL_PERIOD),
        (QUERY_LIMIT, DETAIL_LIMIT),
        (QUERY_MODEL, DETAIL_MODEL),
        (QUERY_STARTING_AFTER, DETAIL_CURSOR),
    ],
    DETAIL_QUERY,
);

#[derive(Debug, garde::Validate)]
pub(super) struct Bounds {
    #[garde(inner(length(bytes, max = PERIOD_MAX_BYTES)))]
    period: Option<String>,
    #[garde(inner(length(bytes, max = TEXT_LEN)))]
    fleet_id: Option<String>,
    #[garde(inner(length(bytes, max = MODEL_ID_MAX_BYTES), custom(nul_free)))]
    model: Option<String>,
    #[garde(inner(length(bytes, max = ACTIVITY_TYPE_MAX_BYTES)))]
    activity_type: Option<String>,
    #[garde(inner(length(bytes, max = TEXT_LEN)))]
    activity_id: Option<String>,
    #[garde(inner(length(bytes, max = INSTANT_MAX_BYTES)))]
    through_at: Option<String>,
    #[garde(inner(length(bytes, max = LIMIT_MAX_BYTES)))]
    limit: Option<String>,
    #[garde(inner(length(bytes, max = CURSOR_MAX_BYTES)))]
    pub(super) starting_after: Option<String>,
}

pub(super) struct Input {
    pub scope: Scope,
    pub request: Request,
    pub position: Option<Position>,
}

pub(super) fn read(
    query: &str,
    tenant: &Uuid7,
    activity: Option<&str>,
    now: Timestamp,
) -> Result<Input, Refusal> {
    let raw = bounded(query, activity)?;
    let cursor = Cursor::parse(&raw)?;
    let period = raw.period.as_deref().unwrap_or(BILLING_PERIOD_MONTH);
    let through = raw
        .through_at
        .as_deref()
        .or_else(|| cursor.as_ref().map(|token| token.scope.through_at.as_str()));
    let window = window(period, through, now)?;
    let fleet = raw.fleet_id.as_deref().map(identifier).transpose()?;
    let kind = work_type(raw.activity_type.as_deref())?;
    let scope = scope(&raw, tenant, window, fleet.as_ref())?;
    if cursor.as_ref().is_some_and(|token| token.scope != scope) {
        return Err(Refusal::malformed(DETAIL_CURSOR));
    }
    let position = cursor.map(|token| token.position);
    let after = activity_boundary(position.as_ref(), activity.is_some())?;
    if activity.is_some() && kind.is_none() {
        return Err(Refusal::malformed(DETAIL_QUERY));
    }
    if let Some(id) = activity {
        identifier(id)?;
    }
    Ok(Input {
        request: Request {
            window,
            filter: Filter {
                fleet,
                model: raw.model.clone(),
                work_type: kind,
            },
            limit: scope.limit,
            after,
        },
        scope,
        position,
    })
}

fn scope(
    raw: &garde::Valid<Bounds>,
    tenant: &Uuid7,
    window: Window,
    fleet: Option<&Uuid7>,
) -> Result<Scope, Refusal> {
    Ok(Scope {
        tenant: tenant.as_str().to_owned(),
        period: raw
            .period
            .as_deref()
            .unwrap_or(BILLING_PERIOD_MONTH)
            .to_owned(),
        fleet_id: fleet.map(|id| id.as_str().to_owned()),
        model: raw.model.clone(),
        activity_type: raw.activity_type.clone(),
        activity_id: raw.activity_id.clone(),
        through_at: window.through.to_string(),
        limit: Limit::parse(raw.limit.as_deref(), CEILING)
            .map_err(|_break| Refusal::malformed(DETAIL_LIMIT))?,
    })
}

fn bounded(query: &str, activity: Option<&str>) -> Result<garde::Valid<Bounds>, Refusal> {
    let value = |name| {
        decoded_parameter(query, name)
            .map(|text| {
                text.filter(|text| !text.is_empty())
                    .map(std::borrow::Cow::into_owned)
            })
            .map_err(|_escape| Refusal::malformed(DETAIL_QUERY))
    };
    garde::Unvalidated::new(Bounds {
        period: value(QUERY_PERIOD)?,
        fleet_id: value("fleet_id")?,
        model: value(QUERY_MODEL)?,
        activity_type: value("activity_type")?,
        activity_id: activity.map(str::to_owned),
        through_at: value("through_at")?,
        limit: value(QUERY_LIMIT)?,
        starting_after: value(QUERY_STARTING_AFTER)?,
    })
    .validate()
    .map_err(|report| Refusal::malformed(SENTENCES.pick(&report)))
}

fn window(period: &str, through: Option<&str>, now: Timestamp) -> Result<Window, Refusal> {
    let through = through.map_or_else(
        || {
            Timestamp::from_millisecond(now.as_millisecond())
                .map_err(|_range| Refusal::malformed(DETAIL_QUERY))
        },
        |text| afd_wire::instant::parse(text).map_err(|_instant| Refusal::malformed(DETAIL_QUERY)),
    )?;
    if through > now {
        return Err(Refusal::malformed(DETAIL_QUERY));
    }
    let date = through.to_zoned(TimeZone::UTC).date();
    let date = match period {
        BILLING_PERIOD_MONTH => date.first_of_month(),
        BILLING_PERIOD_TODAY => date,
        _ => return Err(Refusal::malformed(DETAIL_PERIOD)),
    };
    let from = date
        .at(0, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .map_err(|_range| Refusal::malformed(DETAIL_QUERY))?
        .timestamp();
    Ok(Window { from, through })
}

pub(super) fn identifier(text: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(text).map_err(|_identifier| Refusal::malformed(DETAIL_QUERY))
}

pub(super) fn work_type(kind: Option<&str>) -> Result<Option<&'static str>, Refusal> {
    match kind {
        None => Ok(None),
        Some(BILLING_ACTIVITY_RUNTIME) => Ok(Some(work::FLEET_LEASE)),
        Some(BILLING_ACTIVITY_SETUP) => Ok(Some(work::AUTHORING_ATTEMPT)),
        Some(_) => Err(Refusal::malformed(DETAIL_QUERY)),
    }
}

fn activity_boundary(
    position: Option<&Position>,
    details: bool,
) -> Result<Option<Boundary>, Refusal> {
    match (position, details) {
        (None, _) | (Some(Position::Charge { .. }), true) => Ok(None),
        (
            Some(Position::Activity {
                at,
                id,
                work_type: kind,
            }),
            false,
        ) => {
            let kind = match kind.as_str() {
                work::FLEET_LEASE => work::FLEET_LEASE,
                work::AUTHORING_ATTEMPT => work::AUTHORING_ATTEMPT,
                _ => return Err(Refusal::malformed(DETAIL_CURSOR)),
            };
            Ok(Some(Boundary {
                recorded_at: afd_wire::instant::parse(at)
                    .map_err(|_instant| Refusal::malformed(DETAIL_CURSOR))?,
                work_id: identifier(id)?,
                work_type: kind,
            }))
        }
        _ => Err(Refusal::malformed(DETAIL_CURSOR)),
    }
}

#[cfg(test)]
mod tests;
