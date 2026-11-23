//! No buffered private frame bypasses a fresh membership and generation check.

use af_core::id::Uuid7;
use afd_setup::{Access, Drafts, Observation};
use axum::response::sse::Event;
use futures_util::{Stream, stream};
use std::{collections::HashMap, convert::Infallible};
use tokio::time::Instant;

const FAILED: &str = "draft_stream_recheck_failed";

struct Watch {
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: i64,
    seen: HashMap<String, String>,
    /// The last read found nothing new: wait for a notice before reading again.
    settled: bool,
    done: bool,
    /// Live progress; reopened after a loss, never after the bus closed.
    link: afd_sse::Link,
    /// A subscription was lost and the viewer is not yet told to resync.
    lost: bool,
    /// The viewer was told; a further loss before any delivery is the same
    /// outage and is not announced again.
    resynced: bool,
    /// The account's members changed: read again, which re-proves the viewer.
    members: afd_sse::Link,
    _slot: afd_sse::StreamPermit,
}

/// What a draft stream reads live progress through.
pub(super) struct Feed {
    /// This stream's place under the instance's ceiling.
    pub(super) slot: afd_sse::StreamPermit,
    /// The subscription the handler opened before answering.
    pub(super) link: afd_sse::Link,
    /// The account's members notice, opened beside it.
    pub(super) members: afd_sse::Link,
}

pub(super) fn frames(
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: i64,
    feed: Feed,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let Feed {
        slot,
        link,
        members,
    } = feed;
    stream::unfold(
        Watch {
            drafts,
            workspace,
            subject,
            draft,
            generation,
            seen: HashMap::new(),
            settled: false,
            done: false,
            lost: false,
            resynced: false,
            link,
            members,
            _slot: slot,
        },
        step,
    )
}

/// The next frame. No timer reads Postgres: the stream reads when a notice, a gap
/// or a lag arrives on the draft's subject, when the account's members change, or
/// when a lost subscription reopens; its heartbeat reads nothing.
async fn step(mut watch: Watch) -> Option<(Result<Event, Infallible>, Watch)> {
    let heartbeat_at = Instant::now() + afd_sse::HEARTBEAT_INTERVAL;
    while !watch.done {
        if let Some(event) = recover(&mut watch).await {
            return Some((Ok(event), watch));
        }
        let pending = match wait(&mut watch, heartbeat_at).await {
            Waited::Read(pending) => pending,
            // A lost subscription's retry is due: `recover` reopens it, and a reopen
            // that succeeds owes the read.
            Waited::Retry => continue,
            Waited::Heartbeat => return Some((Ok(heartbeat()), watch)),
            // The bus shut down: end, so the client reconnects to a serving replica.
            Waited::Closed => {
                tracing::debug!(event = "draft_stream_bus_closed");
                return None;
            }
        };
        let access = Access {
            workspace: &watch.workspace,
            subject: &watch.subject,
        };
        match observe(&watch.drafts, access, &watch.draft, watch.generation).await {
            Ok(Observation::Deleted) => {
                watch.done = true;
                return Some((Ok(deleted(&watch.draft)?), watch));
            }
            Ok(Observation::Current(view)) => {
                watch.generation = view.generation;
                if let Some(pending) = pending {
                    // A gap or a lag may have hidden changes: after the resync, read
                    // again at once rather than wait for the next notice.
                    let missed = matches!(
                        pending,
                        afd_bus::Received::Gap | afd_bus::Received::Lagged(_)
                    );
                    if let Some(event) = preview(&watch.draft, &view, pending) {
                        watch.settled = !missed;
                        return Some((Ok(event), watch));
                    }
                }
                if let Some(event) = changed(&mut watch, view.notices) {
                    return Some((Ok(event), watch));
                }
                if view.closed {
                    return None;
                }
                watch.settled = true;
            }
            Err(error) => {
                let error_code = error.code().as_str();
                tracing::info!(event = FAILED, error_code, "private stream access ended");
                return None;
            }
        }
    }
    None
}

/// The fresh membership and generation read every frame waits on. Borrows
/// only the fields it reads: a shared borrow of the watch's link is not `Send`.
async fn observe(
    drafts: &Drafts,
    access: Access<'_>,
    draft: &Uuid7,
    generation: i64,
) -> afd_setup::Result<Observation> {
    drafts.observe(access, draft, generation).await
}

fn heartbeat() -> Event {
    Event::default()
        .event(afd_sse::HEARTBEAT_EVENT)
        .data(afd_sse::HEARTBEAT_DATA)
}

/// Reopen a lost subscription once its retry is due, and tell the viewer to
/// resync after a loss. A closed bus is never retried.
async fn recover(watch: &mut Watch) -> Option<Event> {
    if watch.link.reopen_if_due().await {
        // What was published while the subscription was lost is unknown: read again.
        watch.settled = false;
    }
    if !watch.lost {
        return None;
    }
    // A lost subscription may have dropped progress; the viewer reloads.
    watch.lost = false;
    watch.resynced = true;
    tracing::debug!(event = "draft_stream_resynced");
    Some(resync())
}

fn resync() -> Event {
    Event::default().event("draft.resync").data("{}")
}

fn deleted(draft: &Uuid7) -> Option<Event> {
    Event::default()
        .event("draft.deleted")
        .json_data(serde_json::json!({"draft_id": draft}))
        .ok()
}

/// What ended a wait.
enum Waited {
    /// Read the draft again; with the message that asked, if one did.
    Read(Option<afd_bus::Received>),
    /// The heartbeat fell due with nothing to read.
    Heartbeat,
    /// A lost subscription's retry fell due.
    Retry,
    /// The bus shut down.
    Closed,
}

/// All paths return to the authorization read before they emit anything.
async fn wait(watch: &mut Watch, heartbeat_at: Instant) -> Waited {
    if !watch.settled {
        return Waited::Read(None);
    }
    if watch.link.is_closed() {
        return Waited::Closed;
    }
    let retry_at = watch.link.retry_at();
    let Some(subscription) = watch.link.subscription() else {
        // Lost and retrying: what was published meanwhile is unknown, so `recover`
        // reads again once it reopens the subscription.
        return tokio::select! {
            () = tokio::time::sleep_until(heartbeat_at) => Waited::Heartbeat,
            () = afd_sse::sleep_until_some(retry_at) => Waited::Retry,
            wake = watch.members.changed() => members(wake),
        };
    };
    tokio::select! {
        () = tokio::time::sleep_until(heartbeat_at) => Waited::Heartbeat,
        wake = watch.members.changed() => members(wake),
        received = subscription.recv() => match received {
            Ok(received) => {
                watch.resynced = false;
                Waited::Read(Some(received))
            }
            Err(error) => {
                watch.link.lose(&error);
                watch.lost = !watch.resynced;
                Waited::Read(None)
            }
        },
    }
}

/// A members notice asks for a read, which re-proves the viewer.
const fn members(wake: afd_sse::Wake) -> Waited {
    match wake {
        afd_sse::Wake::Changed => Waited::Read(None),
        afd_sse::Wake::Closed => Waited::Closed,
    }
}

#[derive(garde::Validate)]
struct Payload<'a> {
    // A text piece can expand sixfold when escaped, before JSON frame metadata.
    #[garde(length(bytes, max = afd_bus::DRAFT_FRAME_BYTES))]
    value: &'a str,
}

fn preview(draft: &Uuid7, view: &afd_setup::View, received: afd_bus::Received) -> Option<Event> {
    use afd_bus::Received;
    match received {
        Received::Gap | Received::Lagged(_) => Some(resync()),
        Received::Message(message) => {
            let bounded = garde::Unvalidated::new(Payload {
                value: &message.payload,
            })
            .validate()
            .ok()?;
            // A change notice carries no frame of its own: the read it asked for
            // finds what changed.
            if serde_json::from_str::<afd_setup::Changed>(bounded.value).is_ok() {
                return None;
            }
            if let Ok(frame) = serde_json::from_str::<afd_setup::ContextNotice>(bounded.value) {
                let frame = garde::Unvalidated::new(frame).validate().ok()?;
                if !permitted(draft, view, &frame.draft_id, &frame.sequence) {
                    return None;
                }
                return Event::default()
                    .event(afd_setup::CONTEXT_EVENT)
                    .json_data(&*frame)
                    .ok();
            }
            let frame: afd_setup::Delta = serde_json::from_str(bounded.value).ok()?;
            let frame = garde::Unvalidated::new(frame).validate().ok()?;
            if !permitted(draft, view, &frame.draft_id, &frame.sequence) {
                return None;
            }
            Event::default()
                .event(afd_setup::DELTA_EVENT)
                .json_data(&*frame)
                .ok()
        }
    }
}

fn permitted(draft: &Uuid7, view: &afd_setup::View, subject: &Uuid7, sequence: &str) -> bool {
    sequence.parse::<i64>().is_ok_and(|sequence| {
        subject == draft && !view.closed && view.running_sequence == Some(sequence)
    })
}

fn changed(watch: &mut Watch, notices: Vec<afd_setup::Notice>) -> Option<Event> {
    let live: std::collections::HashSet<_> = notices.iter().map(key).collect();
    watch.seen.retain(|key, _| live.contains(key));
    for notice in notices {
        let key = key(&notice);
        let body = serde_json::to_string(&notice).ok()?;
        if watch.seen.get(&key) != Some(&body) {
            watch.seen.insert(key, body.clone());
            // Something changed: read again at once before waiting.
            watch.settled = false;
            return Some(Event::default().event(notice.event).data(body));
        }
    }
    None
}

fn key(notice: &afd_setup::Notice) -> String {
    format!(
        "{}:{}",
        notice.event,
        notice.sequence.as_deref().unwrap_or_default()
    )
}
