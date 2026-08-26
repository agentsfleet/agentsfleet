use std::collections::HashMap;

use afd_core::id::Uuid7;
use afd_fleet::{KeysetCursor, PageLimit};

const QUERY_LIMIT: &str = "limit";
const QUERY_STARTING_AFTER: &str = "starting_after";
const QUERY_PAGE: &str = "page";
const QUERY_PAGE_SIZE: &str = "page_size";
const QUERY_SORT: &str = "sort";

pub(super) const DETAIL_BAD_PAGE: &str = "limit must be an integer between 1 and 100; starting_after must be a cursor from a previous page";
pub(super) const DETAIL_RETIRED_PAGE: &str =
    "page, page_size and sort are retired; page with starting_after and limit";
pub(super) const DETAIL_BAD_RUNNER_ID: &str = "runner_id must be a valid UUIDv7";

pub(super) struct PageQuery {
    pub(super) cursor: Option<KeysetCursor>,
    pub(super) limit: PageLimit,
}

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
}
