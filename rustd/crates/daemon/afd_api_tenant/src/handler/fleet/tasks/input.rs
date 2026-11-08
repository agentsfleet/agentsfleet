//! Keyset paging uses the existing cursor format and declared query bounds.

use af_core::{id::Uuid7, paging::Cursor};
use af_validate::{Ceiling, Limit};
use afd_fleet::task::TaskAfter;
use afd_http::handler::Refusal;
use serde::Deserialize;

use crate::handler::decoded_parameter;

const DETAIL_CURSOR: &str = "Invalid task page cursor";
const DETAIL_LIMIT: &str = "Task page limit must be between 1 and 100";
const DETAIL_TASK: &str = "task_id must be a UUIDv7";
const DETAIL_QUERY: &str = "Task page query exceeds its supported size";
const PAGE: Ceiling = Ceiling::new(100, 20);

#[derive(Deserialize)]
pub(crate) struct TaskPath {
    pub fleet_id: String,
    pub task_id: String,
}

#[derive(garde::Validate)]
struct PageInput<'a> {
    #[garde(length(bytes, max = 1024))]
    query: &'a str,
}

pub(super) fn page(query: &str) -> Result<(Option<TaskAfter>, u32), Refusal> {
    let raw = garde::Unvalidated::new(PageInput { query })
        .validate()
        .map_err(|_invalid| Refusal::malformed(DETAIL_QUERY))?;
    let asked = decoded_parameter(raw.query, "limit")
        .map_err(|_invalid| Refusal::malformed(DETAIL_LIMIT))?;
    let limit = Limit::parse(asked.as_deref(), PAGE)
        .map_err(|_invalid| Refusal::malformed(DETAIL_LIMIT))?;
    let after = boundary(&raw)?;
    Ok((after, limit))
}

pub(super) fn task(value: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(value).map_err(|_invalid| Refusal::malformed(DETAIL_TASK))
}

fn boundary(input: &garde::Valid<PageInput<'_>>) -> Result<Option<TaskAfter>, Refusal> {
    let Some(value) = decoded_parameter(input.query, "starting_after")
        .map_err(|_invalid| Refusal::malformed(DETAIL_CURSOR))?
    else {
        return Ok(None);
    };
    let Cursor::Timestamp { at, id } =
        Cursor::parse(&value).map_err(|_invalid| Refusal::malformed(DETAIL_CURSOR))?
    else {
        return Err(Refusal::malformed(DETAIL_CURSOR));
    };
    let id = Uuid7::parse(&id).map_err(|_invalid| Refusal::malformed(DETAIL_CURSOR))?;
    Ok(Some(TaskAfter { created_at: at, id }))
}
