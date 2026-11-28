//! The schedules, messages, fleets, triggers and access verbs, typed: the
//! paths under one held run, the bodies carrying its fence, and the query a
//! read or a delete carries it in instead.
//!
//! Split from [`super`] because these are the run's verbs a run's tools
//! drive, where the rest are the supervisor's own duties.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_core::paging::{QUERY_LIMIT, QUERY_STARTING_AFTER};
use af_crypto::entropy::Entropy;
use af_wire::message_verb::MessageRequest;
use af_wire::paths;
use af_wire::schedule_verb::{
    QUERY_FENCING_TOKEN, ScheduleCreateRequest, SchedulePatchRequest, ScheduleRunRequest,
};
use af_wire::steer_verb::FleetSteerRequest;
use afr_tools::FleetsCall;
use afr_tools::ScheduleCall;
use serde::Serialize;
use url::form_urlencoded::Serializer;

use super::{Body, ControlPlane, Verb, run_path};
use crate::error::Result;

mod access;
mod web;

impl ControlPlane {
    /// Sends one schedules call for the fleet `run_id` runs, fenced by
    /// `fencing_token`.
    ///
    /// # Errors
    /// A refusal, or a transport failure. A create, an edit and a run-now
    /// name their call's item and are retried as a blip allows; a read or a
    /// delete is one attempt, and the model decides whether to try again.
    pub(crate) async fn schedules(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call: ScheduleCall<'_>,
    ) -> Result<Body> {
        let collection = run_path(run_id, paths::RUN_SCHEDULES_SUFFIX);
        match call {
            ScheduleCall::Create {
                call_number,
                cron,
                timezone,
                message,
                once,
            } => {
                let item_id = minted()?;
                let item = Item {
                    fencing_token,
                    id: &item_id,
                    call_number,
                };
                let body = created(item, cron, timezone, message, once);
                self.once(Verb::ScheduleCreate, collection, &body).await
            }
            ScheduleCall::List => {
                let path = fenced(&collection, fencing_token, Page::default());
                self.send(Verb::ScheduleList, path, None).await
            }
            ScheduleCall::Update {
                call_number,
                schedule,
                cron,
                timezone,
                message,
                paused,
            } => {
                let path = member(&collection, schedule, None);
                let item_id = minted()?;
                let item = Item {
                    fencing_token,
                    id: &item_id,
                    call_number,
                };
                let body = patched(item, cron, timezone, message, paused);
                self.once(Verb::ScheduleUpdate, path, &body).await
            }
            ScheduleCall::Delete { schedule } => {
                let path = member(&collection, schedule, None);
                let path = fenced(&path, fencing_token, Page::default());
                self.send(Verb::ScheduleDelete, path, None).await
            }
            ScheduleCall::Run {
                call_number,
                schedule,
            } => {
                let path = member(&collection, schedule, Some(paths::SCHEDULE_RUNS_SUFFIX));
                let item_id = minted()?;
                let body = ScheduleRunRequest {
                    fencing_token,
                    item_id: Cow::Borrowed(item_id.as_str()),
                    call_number,
                };
                self.once(Verb::ScheduleRun, path, &body).await
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

    /// Posts one line to the thread of the event `run_id` runs, for the
    /// run's call `call_number`.
    ///
    /// # Errors
    /// A refusal, or a transport failure once retries are spent. A retry
    /// names the same item, so a line that landed and lost its answer is
    /// answered again, not posted twice.
    pub(crate) async fn message(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call_number: u64,
        text: &str,
    ) -> Result<Body> {
        let item_id = minted()?;
        let body = MessageRequest {
            fencing_token,
            item_id: Cow::Borrowed(item_id.as_str()),
            call_number,
            text: Cow::Borrowed(text),
        };
        let path = run_path(run_id, paths::RUN_MESSAGES_SUFFIX);
        self.once(Verb::Message, path, &body).await
    }

    /// Sends one effect's `body`, retried as a blip allows: the body names
    /// the call's item, so `agentsfleetd` answers a retry with the first
    /// result and acts once.
    async fn once<T: Serialize + Sync>(
        &self,
        verb: Verb,
        path: Cow<'static, str>,
        body: &T,
    ) -> Result<Body> {
        self.retrying(|| self.send_json(verb, path.clone(), body))
            .await
    }

    /// Reads the workspace's fleets, or one fleet's events, for the fleet
    /// `run_id` runs, fenced by `fencing_token` in the query.
    ///
    /// # Errors
    /// A refusal, or a transport failure; not retried, as no verb a tool
    /// drives is: the model decides whether to try again.
    pub(crate) async fn fleets(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        call: FleetsCall<'_>,
    ) -> Result<Body> {
        let collection = run_path(run_id, paths::RUN_FLEETS_SUFFIX);
        match call {
            FleetsCall::List {
                limit,
                starting_after,
            } => {
                let page = Page {
                    limit,
                    starting_after,
                };
                let path = fenced(&collection, fencing_token, page);
                self.send(Verb::FleetList, path, None).await
            }
            FleetsCall::Events {
                fleet,
                limit,
                starting_after,
            } => {
                let events = member(&collection, fleet, Some(paths::FLEET_TURNS_SUFFIX));
                let page = Page {
                    limit,
                    starting_after,
                };
                let path = fenced(&events, fencing_token, page);
                self.send(Verb::FleetTurns, path, None).await
            }
        }
    }

    /// Puts `text` on `fleet` for the fleet `run_id` runs.
    ///
    /// # Errors
    /// A refusal, or a transport failure; never retried by the supervisor:
    /// a resend of the same words is the model's, and the daemon answers it
    /// with the first event.
    pub(crate) async fn fleet_steer(
        &self,
        run_id: &Uuid7,
        fencing_token: u64,
        fleet: &Uuid7,
        text: &str,
    ) -> Result<Body> {
        let body = FleetSteerRequest {
            fencing_token,
            fleet_id: Cow::Borrowed(fleet.as_str()),
            text: Cow::Borrowed(text),
        };
        let path = run_path(run_id, paths::RUN_STEERS_SUFFIX);
        self.send_json(Verb::FleetSteer, path, &body).await
    }
}

/// The keyset page a runs read asks for; empty for every other read.
#[derive(Debug, Clone, Copy, Default)]
struct Page<'a> {
    limit: Option<u32>,
    starting_after: Option<&'a str>,
}

/// One member under a run's collection — a schedule, or a fleet — and a
/// segment beneath it.
fn member(collection: &str, schedule: &Uuid7, suffix: Option<&str>) -> Cow<'static, str> {
    Cow::Owned(suffix.map_or_else(
        || format!("{collection}/{schedule}"),
        |suffix| format!("{collection}/{schedule}/{suffix}"),
    ))
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

/// The item one effect names: the run's fence, the item minted for the call,
/// and the call's number in its run.
#[derive(Debug, Clone, Copy)]
struct Item<'a> {
    fencing_token: u64,
    id: &'a Uuid7,
    call_number: u64,
}

/// A fresh item id for one call, sent again on each of its retries.
fn minted() -> Result<Uuid7> {
    Ok(Entropy::new().uuid7(af_core::clock::now())?)
}

/// A create's body, fenced and naming its item.
fn created<'a>(
    item: Item<'a>,
    cron: &'a str,
    timezone: Option<&'a str>,
    message: &'a str,
    once: bool,
) -> ScheduleCreateRequest<'a> {
    ScheduleCreateRequest {
        fencing_token: item.fencing_token,
        item_id: Cow::Borrowed(item.id.as_str()),
        call_number: item.call_number,
        cron: Cow::Borrowed(cron),
        timezone: timezone.map(Cow::Borrowed),
        message: Cow::Borrowed(message),
        once,
    }
}

/// A patch's body, fenced and naming its item: each field it names, and
/// nothing else.
fn patched<'a>(
    item: Item<'a>,
    cron: Option<&'a str>,
    timezone: Option<&'a str>,
    message: Option<&'a str>,
    paused: Option<bool>,
) -> SchedulePatchRequest<'a> {
    SchedulePatchRequest {
        fencing_token: item.fencing_token,
        item_id: Cow::Borrowed(item.id.as_str()),
        call_number: item.call_number,
        cron: cron.map(Cow::Borrowed),
        timezone: timezone.map(Cow::Borrowed),
        message: message.map(Cow::Borrowed),
        paused,
    }
}

#[cfg(test)]
mod tests;
