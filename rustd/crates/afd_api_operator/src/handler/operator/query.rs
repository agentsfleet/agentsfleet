//! What the operator plane's runner and lease lists read off a query string.
//!
//! Every `?limit` here is the keyset lists' [`CEILING`], read through
//! `afd_validate::Limit`; each list keeps its own sentence. The two filters
//! with a bound — `?fleet` on leases and the `?event_type` set on runner
//! events — are garde structs proved before anything reads them.

use std::collections::HashMap;

use afd_core::id::Uuid7;
use afd_core::paging::CEILING;
use afd_runner::{KeysetCursor, RunnerEventFilter};
use afd_validate::Limit;
use afd_wire::admin::RunnerEventType;
use garde::Validate as _;

const QUERY_LIMIT: &str = "limit";
const QUERY_STARTING_AFTER: &str = "starting_after";
const QUERY_PAGE: &str = "page";
const QUERY_PAGE_SIZE: &str = "page_size";
const QUERY_SORT: &str = "sort";
const QUERY_WORKSPACE_ID: &str = "workspace_id";
const QUERY_FLEET: &str = "fleet";
const QUERY_EVENT_TYPE: &str = "event_type";
const QUERY_SINCE: &str = "since";
const QUERY_UNTIL: &str = "until";
const MAX_FLEET_FILTER_LEN: usize = 200;
const MAX_EVENT_TYPE_TOKENS: usize = 11;

pub(super) const DETAIL_BAD_PAGE: &str = "limit must be an integer between 1 and 100; starting_after must be a cursor from a previous page";
pub(super) const DETAIL_RETIRED_PAGE: &str =
    "page, page_size and sort are retired; page with starting_after and limit";
pub(super) const DETAIL_BAD_RUNNER_ID: &str = "runner_id must be a valid UUIDv7";

pub(super) struct PageQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: u32,
}

pub(super) struct LeaseQuery {
    pub(super) starting_after: Option<Uuid7>,
    pub(super) workspace: Option<Uuid7>,
    pub(super) fleet: Option<String>,
    pub(super) limit: u32,
}

pub(super) struct EventQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: u32,
    pub(super) filter: RunnerEventFilter,
}

/// The `?fleet` filter on the lease list: an id or a name, bounded.
#[derive(garde::Validate)]
struct FleetFilter<'q> {
    #[garde(inner(length(bytes, min = 1, max = MAX_FLEET_FILTER_LEN)))]
    fleet: Option<&'q str>,
}

/// An `?event_type` set split into its tokens, bounded before one is parsed.
#[derive(garde::Validate)]
struct EventTypeTokens<'q> {
    #[garde(
        length(min = 1, max = MAX_EVENT_TYPE_TOKENS),
        inner(length(bytes, min = 1))
    )]
    tokens: Vec<&'q str>,
}

pub(super) const DETAIL_BAD_LEASE_LIMIT: &str = "limit must be an integer between 1 and 100";
pub(super) const DETAIL_BAD_LEASE_CURSOR: &str = "starting_after must be a lease id held by this runner, and must match workspace_id and fleet when those filters are set";
pub(super) const DETAIL_BAD_WORKSPACE: &str = "workspace_id must be a workspace id";
pub(super) const DETAIL_BAD_FLEET: &str =
    "fleet must be a fleet id or name, at most 200 characters";
pub(super) const DETAIL_BAD_EVENTS: &str = "limit must be between 1 and 100; starting_after must be a cursor from a previous page; event_type must be a comma-separated set of runner event types; since/until must be millis";
pub(super) const DETAIL_RETIRED_EVENT_PAGE: &str =
    "page and page_size are retired on this list; page with starting_after and limit";

pub(super) fn page(params: &HashMap<String, String>) -> Result<PageQuery, &'static str> {
    if [QUERY_PAGE, QUERY_PAGE_SIZE, QUERY_SORT]
        .iter()
        .any(|key| params.contains_key(*key))
    {
        return Err(DETAIL_RETIRED_PAGE);
    }
    let limit = requested_limit(params, DETAIL_BAD_PAGE)?;
    let cursor = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| cursor(raw))
        .transpose()?;
    Ok(PageQuery { cursor, limit })
}

pub(super) fn runner_id(raw: &str) -> Result<Uuid7, &'static str> {
    Uuid7::parse(raw).map_err(|_invalid| DETAIL_BAD_RUNNER_ID)
}

pub(super) fn leases(params: &HashMap<String, String>) -> Result<LeaseQuery, &'static str> {
    let limit = requested_limit(params, DETAIL_BAD_LEASE_LIMIT)?;
    let starting_after = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| DETAIL_BAD_LEASE_CURSOR))
        .transpose()?;
    let workspace = params
        .get(QUERY_WORKSPACE_ID)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| DETAIL_BAD_WORKSPACE))
        .transpose()?;
    let filter = FleetFilter {
        fleet: params.get(QUERY_FLEET).map(String::as_str),
    };
    filter.validate().map_err(|_report| DETAIL_BAD_FLEET)?;
    Ok(LeaseQuery {
        starting_after,
        workspace,
        fleet: filter.fleet.map(str::to_owned),
        limit,
    })
}

pub(super) fn events(params: &HashMap<String, String>) -> Result<EventQuery, &'static str> {
    if [QUERY_PAGE, QUERY_PAGE_SIZE]
        .iter()
        .any(|key| params.contains_key(*key))
    {
        return Err(DETAIL_RETIRED_EVENT_PAGE);
    }
    let limit = requested_limit(params, DETAIL_BAD_EVENTS)?;
    let cursor = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| cursor(raw).map_err(|_detail| DETAIL_BAD_EVENTS))
        .transpose()?;
    let event_types = params
        .get(QUERY_EVENT_TYPE)
        .map(|raw| event_types(raw))
        .transpose()?
        .unwrap_or_default();
    let since = optional_i64(params.get(QUERY_SINCE))?;
    let until = optional_i64(params.get(QUERY_UNTIL))?;
    let filter = RunnerEventFilter::new(event_types, since, until).ok_or(DETAIL_BAD_EVENTS)?;
    Ok(EventQuery {
        cursor,
        limit,
        filter,
    })
}

pub(super) fn format(cursor: &KeysetCursor) -> String {
    format!("{}:{}", cursor.created_at(), cursor.id())
}

fn cursor(raw: &str) -> Result<KeysetCursor, &'static str> {
    let (created_at, id) = raw.split_once(':').ok_or(DETAIL_BAD_PAGE)?;
    let created_at = created_at
        .parse::<i64>()
        .map_err(|_invalid| DETAIL_BAD_PAGE)?;
    let id = Uuid7::parse(id).map_err(|_invalid| DETAIL_BAD_PAGE)?;
    Ok(KeysetCursor::new(created_at, id))
}

/// The page size a list asked for, inside the keyset lists' ceiling, or that
/// list's own sentence.
fn requested_limit(
    params: &HashMap<String, String>,
    detail: &'static str,
) -> Result<u32, &'static str> {
    Limit::parse(params.get(QUERY_LIMIT).map(String::as_str), CEILING).map_err(|_break| detail)
}

/// The event types a set names: split, bounded, then each token read.
fn event_types(raw: &str) -> Result<Vec<RunnerEventType>, &'static str> {
    let tokens = garde::Unvalidated::new(EventTypeTokens {
        tokens: raw.split(',').collect(),
    })
    .validate()
    .map_err(|_report| DETAIL_BAD_EVENTS)?;
    parse_event_types(&tokens)
}

/// Each token of a set already inside its bounds, as a runner event type.
fn parse_event_types(
    set: &garde::Valid<EventTypeTokens<'_>>,
) -> Result<Vec<RunnerEventType>, &'static str> {
    set.tokens
        .iter()
        .map(|token| {
            serde_json::from_value(serde_json::Value::String((*token).to_owned()))
                .map_err(|_invalid| DETAIL_BAD_EVENTS)
        })
        .collect()
}

fn optional_i64(raw: Option<&String>) -> Result<Option<i64>, &'static str> {
    raw.map(|value| value.parse::<i64>().map_err(|_invalid| DETAIL_BAD_EVENTS))
        .transpose()
}

#[cfg(test)]
mod tests;
