//! The schedules and messages verbs, typed: the paths under one held lease,
//! the bodies carrying its fence, and the query a read or a delete carries it
//! in instead.
//!
//! Split from [`super`] because these are the lease's verbs a run's tools
//! drive, where the rest are the supervisor's own duties.

use std::borrow::Cow;

use afd_core::id::Uuid7;
use afd_core::paging::{QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_wire::message_verb::MessageRequest;
use afd_wire::paths;
use afd_wire::schedule_verb::{
    QUERY_FENCING_TOKEN, ScheduleCreateRequest, SchedulePatchRequest, ScheduleRunRequest,
};
use afr_agent::ScheduleCall;
use url::form_urlencoded::Serializer;

use super::{Body, ControlPlane, Verb, lease_path};
use crate::error::Result;

impl ControlPlane {
    /// Sends one schedules call for the fleet `lease_id` runs, fenced by
    /// `fencing_token`.
    ///
    /// # Errors
    /// A refusal, or a transport failure; never retried, because a create is
    /// not idempotent and the model decides whether to try again.
    pub(crate) async fn schedules(
        &self,
        lease_id: &Uuid7,
        fencing_token: u64,
        call: ScheduleCall<'_>,
    ) -> Result<Body> {
        let collection = lease_path(lease_id, paths::LEASE_SCHEDULES_SUFFIX);
        match call {
            ScheduleCall::Create {
                cron,
                timezone,
                message,
                once,
            } => {
                let body = created(fencing_token, cron, timezone, message, once);
                self.send_json(Verb::ScheduleCreate, collection, &body)
                    .await
            }
            ScheduleCall::List => {
                let path = fenced(&collection, fencing_token, Page::default());
                self.send(Verb::ScheduleList, path, None).await
            }
            ScheduleCall::Update {
                schedule,
                cron,
                timezone,
                message,
                paused,
            } => {
                let path = member(&collection, schedule, None);
                let body = patched(fencing_token, cron, timezone, message, paused);
                self.send_json(Verb::ScheduleUpdate, path, &body).await
            }
            ScheduleCall::Delete { schedule } => {
                let path = member(&collection, schedule, None);
                let path = fenced(&path, fencing_token, Page::default());
                self.send(Verb::ScheduleDelete, path, None).await
            }
            ScheduleCall::Run { schedule } => {
                let path = member(&collection, schedule, Some(paths::SCHEDULE_RUNS_SUFFIX));
                let body = ScheduleRunRequest { fencing_token };
                self.send_json(Verb::ScheduleRun, path, &body).await
            }
            ScheduleCall::Runs {
                schedule,
                limit,
                starting_after,
            } => {
                let path = member(&collection, schedule, Some(paths::SCHEDULE_RUNS_SUFFIX));
                let page = Page {
                    limit,
                    starting_after,
                };
                let path = fenced(&path, fencing_token, page);
                self.send(Verb::ScheduleRuns, path, None).await
            }
        }
    }

    /// Posts one line to the thread of the event `lease_id` runs.
    ///
    /// # Errors
    /// A refusal, or a transport failure; never retried, because a line that
    /// landed and lost its answer would be posted twice.
    pub(crate) async fn message(
        &self,
        lease_id: &Uuid7,
        fencing_token: u64,
        text: &str,
    ) -> Result<Body> {
        let body = MessageRequest {
            fencing_token,
            text: Cow::Borrowed(text),
        };
        let path = lease_path(lease_id, paths::LEASE_MESSAGES_SUFFIX);
        self.send_json(Verb::Message, path, &body).await
    }
}

/// The keyset page a runs read asks for; empty for every other read.
#[derive(Debug, Clone, Copy, Default)]
struct Page<'a> {
    limit: Option<u32>,
    starting_after: Option<&'a str>,
}

/// One schedule under the lease's collection, and a segment beneath it.
fn member(collection: &str, schedule: &Uuid7, suffix: Option<&str>) -> Cow<'static, str> {
    Cow::Owned(match suffix {
        Some(suffix) => format!("{collection}/{schedule}/{suffix}"),
        None => format!("{collection}/{schedule}"),
    })
}

/// `path` with the fence, and any page, as its query: encoded, because a page
/// cursor is the model's text.
fn fenced(path: &str, fencing_token: u64, page: Page<'_>) -> Cow<'static, str> {
    let mut query = Serializer::new(String::new());
    query.append_pair(QUERY_FENCING_TOKEN, &fencing_token.to_string());
    if let Some(limit) = page.limit {
        query.append_pair(QUERY_LIMIT, &limit.to_string());
    }
    if let Some(after) = page.starting_after {
        query.append_pair(QUERY_STARTING_AFTER, after);
    }
    Cow::Owned(format!("{path}?{}", query.finish()))
}

/// A create's body, fenced.
fn created<'a>(
    fencing_token: u64,
    cron: &'a str,
    timezone: Option<&'a str>,
    message: &'a str,
    once: bool,
) -> ScheduleCreateRequest<'a> {
    ScheduleCreateRequest {
        fencing_token,
        cron: Cow::Borrowed(cron),
        timezone: timezone.map(Cow::Borrowed),
        message: Cow::Borrowed(message),
        once,
    }
}

/// A patch's body, fenced: each field it names, and nothing else.
fn patched<'a>(
    fencing_token: u64,
    cron: Option<&'a str>,
    timezone: Option<&'a str>,
    message: Option<&'a str>,
    paused: Option<bool>,
) -> SchedulePatchRequest<'a> {
    SchedulePatchRequest {
        fencing_token,
        cron: cron.map(Cow::Borrowed),
        timezone: timezone.map(Cow::Borrowed),
        message: message.map(Cow::Borrowed),
        paused,
    }
}

#[cfg(test)]
#[path = "lease_verbs/tests.rs"]
mod tests;
