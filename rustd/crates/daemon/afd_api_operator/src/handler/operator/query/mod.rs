//! What the operator plane's runner and run lists read off a query string.
//!
//! Every `?limit` here is the deployment's list page (`[api]`), read through
//! `af_validate::Limit`; each list keeps its own sentence, which names it. The two filters
//! with a bound — `?fleet` on runs and the `?event_type` set on runner
//! events — are garde structs proved before anything reads them.

use std::borrow::Cow;
use std::collections::HashMap;

use af_core::id::Uuid7;
use af_core::paging::Cursor;
use af_core::spelling::from_spelling;
use af_validate::{Ceiling, Limit};
use afd_api_wire::admin::RunnerEventType;
use afd_runner::{KeysetCursor, RunnerHistoryFilter};
use garde::Validate as _;
use jiff::Timestamp;

const QUERY_LIMIT: &str = "limit";
const QUERY_STARTING_AFTER: &str = "starting_after";
const QUERY_WORKSPACE_ID: &str = "workspace_id";
const QUERY_FLEET: &str = "fleet";
const QUERY_EVENT_TYPE: &str = "event_type";
const QUERY_SINCE: &str = "since";
const QUERY_UNTIL: &str = "until";
const MAX_FLEET_FILTER_LEN: usize = 200;
/// Between the tokens of an `?event_type` set.
const EVENT_TYPE_SEPARATOR: char = ',';

pub(super) const DETAIL_BAD_RUNNER_ID: &str = "runner_id must be a valid UUIDv7";

pub(super) struct PageQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: u32,
}

pub(super) struct RunQuery {
    pub(super) starting_after: Option<Uuid7>,
    pub(super) workspace: Option<Uuid7>,
    pub(super) fleet: Option<String>,
    pub(super) limit: u32,
}

pub(super) struct EventQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: u32,
    pub(super) filter: RunnerHistoryFilter,
}

/// The `?fleet` filter on the run list: an id or a name, bounded.
#[derive(garde::Validate)]
struct FleetFilter<'q> {
    #[garde(inner(length(bytes, min = 1, max = MAX_FLEET_FILTER_LEN)))]
    fleet: Option<&'q str>,
}

/// An `?event_type` set split into its tokens, each non-empty.
#[derive(garde::Validate)]
struct EventTypeTokens<'q> {
    #[garde(length(min = 1), inner(length(bytes, min = 1)))]
    tokens: Vec<&'q str>,
}

const DETAIL_BAD_RUN_CURSOR: &str = "starting_after must be a run id held by this runner, and must match workspace_id and fleet when those filters are set";
const DETAIL_BAD_WORKSPACE: &str = "workspace_id must be a workspace id";
const DETAIL_BAD_FLEET: &str = "fleet must be a fleet id or name, at most 200 characters";
/// What follows the limit's clause in a runner list's sentence.
const PAGE_CURSOR_CLAUSE: &str = "starting_after must be a cursor from a previous page";
/// What follows the limit's clause in the runner events sentence.
const EVENTS_CLAUSE: &str = "starting_after must be a cursor from a previous page; event_type must be a comma-separated set of runner event types; since/until must be RFC 3339 instants";

/// Which of a list's sentences a query earned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Bad {
    /// A runner list's limit or cursor.
    Page,
    /// A run list's limit.
    RunLimit,
    /// A run list's cursor.
    RunCursor,
    /// A run list's workspace filter.
    Workspace,
    /// A run list's fleet filter.
    Fleet,
    /// Anything the runner events list reads.
    Events,
}

impl Bad {
    /// The sentence the caller reads, naming the list page it was held to.
    pub(super) fn sentence(self, ceiling: Ceiling) -> Cow<'static, str> {
        use afd_http::paging::limit_sentence;
        match self {
            Self::Page => Cow::Owned(format!(
                "{}; {PAGE_CURSOR_CLAUSE}",
                limit_sentence("limit must be an integer", ceiling)
            )),
            Self::RunLimit => Cow::Owned(limit_sentence("limit must be an integer", ceiling)),
            Self::RunCursor => Cow::Borrowed(DETAIL_BAD_RUN_CURSOR),
            Self::Workspace => Cow::Borrowed(DETAIL_BAD_WORKSPACE),
            Self::Fleet => Cow::Borrowed(DETAIL_BAD_FLEET),
            Self::Events => Cow::Owned(format!(
                "{}; {EVENTS_CLAUSE}",
                limit_sentence("limit must be", ceiling)
            )),
        }
    }
}

pub(super) fn page(params: &HashMap<String, String>, ceiling: Ceiling) -> Result<PageQuery, Bad> {
    let limit = requested_limit(params, ceiling, Bad::Page)?;
    let cursor = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| cursor(raw).ok_or(Bad::Page))
        .transpose()?;
    Ok(PageQuery { cursor, limit })
}

pub(super) fn runs(params: &HashMap<String, String>, ceiling: Ceiling) -> Result<RunQuery, Bad> {
    let limit = requested_limit(params, ceiling, Bad::RunLimit)?;
    let starting_after = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| Bad::RunCursor))
        .transpose()?;
    let workspace = params
        .get(QUERY_WORKSPACE_ID)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| Bad::Workspace))
        .transpose()?;
    let filter = FleetFilter {
        fleet: params.get(QUERY_FLEET).map(String::as_str),
    };
    filter.validate().map_err(|_report| Bad::Fleet)?;
    Ok(RunQuery {
        starting_after,
        workspace,
        fleet: filter.fleet.map(str::to_owned),
        limit,
    })
}

pub(super) fn events(
    params: &HashMap<String, String>,
    ceiling: Ceiling,
) -> Result<EventQuery, Bad> {
    let limit = requested_limit(params, ceiling, Bad::Events)?;
    let cursor = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| cursor(raw).ok_or(Bad::Events))
        .transpose()?;
    let event_types = params
        .get(QUERY_EVENT_TYPE)
        .map(|raw| event_types(raw))
        .transpose()?
        .unwrap_or_default();
    let since = optional_instant(params.get(QUERY_SINCE))?;
    let until = optional_instant(params.get(QUERY_UNTIL))?;
    let filter = RunnerHistoryFilter::new(event_types, since, until).ok_or(Bad::Events)?;
    Ok(EventQuery {
        cursor,
        limit,
        filter,
    })
}

pub(super) fn format(cursor: &KeysetCursor) -> String {
    Cursor::Timestamp {
        at: cursor.created_at(),
        id: cursor.id().as_str().to_owned(),
    }
    .to_string()
}

/// A timestamp boundary whose identifier is a version-7 id. The text form
/// parses as a cursor too, and is refused: these lists sort by `created_at`.
fn cursor(raw: &str) -> Option<KeysetCursor> {
    let Ok(Cursor::Timestamp { at, id }) = Cursor::parse(raw) else {
        return None;
    };
    let id = Uuid7::parse(&id).ok()?;
    Some(KeysetCursor::new(at, id))
}

/// The page size a list asked for, inside the list page, or that list's own
/// sentence.
fn requested_limit(
    params: &HashMap<String, String>,
    ceiling: Ceiling,
    bad: Bad,
) -> Result<u32, Bad> {
    Limit::parse(params.get(QUERY_LIMIT).map(String::as_str), ceiling).map_err(|_break| bad)
}

/// The event types a set names: split, then each token read.
fn event_types(raw: &str) -> Result<Vec<RunnerEventType>, Bad> {
    let tokens = garde::Unvalidated::new(EventTypeTokens {
        tokens: split_tokens(raw),
    })
    .validate()
    .map_err(|_report| Bad::Events)?;
    parse_event_types(&tokens)
}

/// A set's tokens; the request head bounds how many a query carries.
fn split_tokens(raw: &str) -> Vec<&str> {
    raw.split(EVENT_TYPE_SEPARATOR).collect()
}

/// Each token of a set already inside its bounds, as a runner event type.
fn parse_event_types(set: &garde::Valid<EventTypeTokens<'_>>) -> Result<Vec<RunnerEventType>, Bad> {
    set.tokens
        .iter()
        .map(|token| from_spelling(token).ok_or(Bad::Events))
        .collect()
}

fn optional_instant(raw: Option<&String>) -> Result<Option<Timestamp>, Bad> {
    raw.map(|value| af_wire::instant::parse(value).map_err(|_invalid| Bad::Events))
        .transpose()
}

#[cfg(test)]
mod tests;
