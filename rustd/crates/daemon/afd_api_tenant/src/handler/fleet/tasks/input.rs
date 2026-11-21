//! Keyset paging uses the existing cursor format and declared query bounds.

use af_core::{id::Uuid7, paging::Cursor};
use af_validate::{Ceiling, Limit};
use afd_fleet::task::TaskAfter;
use afd_http::handler::Refusal;
use serde::Deserialize;

use crate::handler::decoded_parameter;

const DETAIL_CURSOR: &str = "Invalid task page cursor";
const DETAIL_TASK: &str = "task_id must be a UUIDv7";

#[derive(Deserialize)]
pub(crate) struct TaskPath {
    pub fleet_id: String,
    pub task_id: String,
}

/// The page a task list asks for, inside the deployment's `[api]` list page.
pub(super) fn page(query: &str, ceiling: Ceiling) -> Result<(Option<TaskAfter>, u32), Refusal> {
    let refused = || {
        Refusal::malformed_sentence(afd_http::paging::limit_sentence(
            "Task page limit must be",
            ceiling,
        ))
    };
    let asked = decoded_parameter(query, "limit").map_err(|_invalid| refused())?;
    let limit = Limit::parse(asked.as_deref(), ceiling).map_err(|_invalid| refused())?;
    let after = boundary(query)?;
    Ok((after, limit))
}

pub(super) fn task(value: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(value).map_err(|_invalid| Refusal::malformed(DETAIL_TASK))
}

fn boundary(query: &str) -> Result<Option<TaskAfter>, Refusal> {
    let Some(value) = decoded_parameter(query, "starting_after")
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
