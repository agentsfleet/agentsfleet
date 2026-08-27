use std::collections::HashMap;

use afd_core::id::Uuid7;
use afd_fleet::{KeysetCursor, PageLimit, RunnerEventFilter};
use afd_wire::admin::RunnerEventType;

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
    pub(super) limit: PageLimit,
}

pub(super) struct LeaseQuery {
    pub(super) starting_after: Option<Uuid7>,
    pub(super) workspace: Option<Uuid7>,
    pub(super) fleet: Option<String>,
    pub(super) limit: u32,
}

pub(super) struct EventQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: PageLimit,
    pub(super) filter: RunnerEventFilter,
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
    let limit = match params.get(QUERY_LIMIT) {
        Some(raw) => raw
            .parse::<u32>()
            .ok()
            .and_then(PageLimit::new)
            .ok_or(DETAIL_BAD_PAGE)?,
        None => PageLimit::default(),
    };
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
    let limit = match params.get(QUERY_LIMIT).map(String::as_str) {
        None | Some("") => 50,
        Some(raw) => raw
            .parse::<u32>()
            .ok()
            .filter(|limit| (1..=100).contains(limit))
            .ok_or(DETAIL_BAD_LEASE_LIMIT)?,
    };
    let starting_after = params
        .get(QUERY_STARTING_AFTER)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| DETAIL_BAD_LEASE_CURSOR))
        .transpose()?;
    let workspace = params
        .get(QUERY_WORKSPACE_ID)
        .map(|raw| Uuid7::parse(raw).map_err(|_invalid| DETAIL_BAD_WORKSPACE))
        .transpose()?;
    let fleet = params.get(QUERY_FLEET).cloned();
    if fleet
        .as_ref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_FLEET_FILTER_LEN)
    {
        return Err(DETAIL_BAD_FLEET);
    }
    Ok(LeaseQuery {
        starting_after,
        workspace,
        fleet,
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
    let limit = match params.get(QUERY_LIMIT) {
        Some(raw) => raw
            .parse::<u32>()
            .ok()
            .and_then(PageLimit::new)
            .ok_or(DETAIL_BAD_EVENTS)?,
        None => PageLimit::default(),
    };
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

fn event_types(raw: &str) -> Result<Vec<RunnerEventType>, &'static str> {
    if raw.is_empty() {
        return Err(DETAIL_BAD_EVENTS);
    }
    let tokens = raw.split(',').collect::<Vec<_>>();
    if tokens.len() > MAX_EVENT_TYPE_TOKENS || tokens.iter().any(|token| token.is_empty()) {
        return Err(DETAIL_BAD_EVENTS);
    }
    tokens
        .into_iter()
        .map(|token| {
            serde_json::from_value(serde_json::Value::String(token.to_owned()))
                .map_err(|_invalid| DETAIL_BAD_EVENTS)
        })
        .collect()
}

fn optional_i64(raw: Option<&String>) -> Result<Option<i64>, &'static str> {
    raw.map(|value| value.parse::<i64>().map_err(|_invalid| DETAIL_BAD_EVENTS))
        .transpose()
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "tests inspect canonical query fixtures")]
mod tests {
    use super::*;

    #[test]
    fn page_query_accepts_defaults_and_composite_cursors() {
        let empty = HashMap::new();
        let defaults = page(&empty).expect("defaults are valid");
        assert_eq!(defaults.limit, PageLimit::default());
        assert!(defaults.cursor.is_none());

        let mut params = HashMap::new();
        params.insert(QUERY_LIMIT.to_owned(), "100".to_owned());
        let cursor = "1725000000000:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
        params.insert(QUERY_STARTING_AFTER.to_owned(), cursor.to_owned());
        let parsed = page(&params).expect("the boundary is canonical");
        assert_eq!(parsed.limit.get(), 100);
        assert_eq!(
            format(parsed.cursor.as_ref().expect("cursor exists")),
            cursor
        );
    }

    #[test]
    fn page_query_refuses_retired_and_out_of_range_inputs() {
        for (key, value, detail) in [
            (QUERY_PAGE, "2", DETAIL_RETIRED_PAGE),
            (QUERY_LIMIT, "0", DETAIL_BAD_PAGE),
            (QUERY_LIMIT, "101", DETAIL_BAD_PAGE),
            (QUERY_STARTING_AFTER, "not-a-cursor", DETAIL_BAD_PAGE),
        ] {
            let params = HashMap::from([(key.to_owned(), value.to_owned())]);
            assert_eq!(page(&params).err(), Some(detail));
        }
    }

    #[test]
    fn lease_query_parses_independent_filters_and_refuses_each_bad_dimension() {
        let cursor = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
        let workspace = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a7d";
        let params = HashMap::from([
            (QUERY_LIMIT.to_owned(), "100".to_owned()),
            (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
            (QUERY_WORKSPACE_ID.to_owned(), workspace.to_owned()),
            (QUERY_FLEET.to_owned(), "Production".to_owned()),
        ]);
        let parsed = leases(&params).expect("every dimension is valid");
        assert_eq!(parsed.limit, 100);
        assert_eq!(
            parsed.starting_after.as_ref().map(Uuid7::as_str),
            Some(cursor)
        );
        assert_eq!(
            parsed.workspace.as_ref().map(Uuid7::as_str),
            Some(workspace)
        );
        assert_eq!(parsed.fleet.as_deref(), Some("Production"));

        for (key, value, detail) in [
            (QUERY_LIMIT, "0", DETAIL_BAD_LEASE_LIMIT),
            (QUERY_STARTING_AFTER, "foreign", DETAIL_BAD_LEASE_CURSOR),
            (QUERY_WORKSPACE_ID, "workspace", DETAIL_BAD_WORKSPACE),
            (QUERY_FLEET, "", DETAIL_BAD_FLEET),
        ] {
            assert_eq!(
                leases(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
                Some(detail)
            );
        }
    }

    #[test]
    fn event_query_parses_sets_and_windows_and_refuses_partial_shapes() {
        let cursor = "1725000000000:0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
        let params = HashMap::from([
            (QUERY_LIMIT.to_owned(), "2".to_owned()),
            (QUERY_STARTING_AFTER.to_owned(), cursor.to_owned()),
            (
                QUERY_EVENT_TYPE.to_owned(),
                "runner_online,runner_offline".to_owned(),
            ),
            (QUERY_SINCE.to_owned(), "10".to_owned()),
            (QUERY_UNTIL.to_owned(), "20".to_owned()),
        ]);
        let parsed = events(&params).expect("the event query is valid");
        assert_eq!(parsed.limit.get(), 2);
        assert_eq!(parsed.cursor.as_ref().map(format).as_deref(), Some(cursor));

        for (key, value, detail) in [
            (QUERY_PAGE, "2", DETAIL_RETIRED_EVENT_PAGE),
            (QUERY_PAGE_SIZE, "10", DETAIL_RETIRED_EVENT_PAGE),
            (QUERY_LIMIT, "0", DETAIL_BAD_EVENTS),
            (QUERY_STARTING_AFTER, "not-a-cursor", DETAIL_BAD_EVENTS),
            (QUERY_EVENT_TYPE, "", DETAIL_BAD_EVENTS),
            (QUERY_EVENT_TYPE, "runner_online,", DETAIL_BAD_EVENTS),
            (QUERY_EVENT_TYPE, "not_an_event", DETAIL_BAD_EVENTS),
            (QUERY_SINCE, "yesterday", DETAIL_BAD_EVENTS),
        ] {
            assert_eq!(
                events(&HashMap::from([(key.to_owned(), value.to_owned())])).err(),
                Some(detail)
            );
        }
        assert_eq!(
            events(&HashMap::from([
                (QUERY_SINCE.to_owned(), "21".to_owned()),
                (QUERY_UNTIL.to_owned(), "20".to_owned()),
            ]))
            .err(),
            Some(DETAIL_BAD_EVENTS)
        );
    }
}
