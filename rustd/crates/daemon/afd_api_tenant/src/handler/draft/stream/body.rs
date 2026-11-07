//! No buffered private frame bypasses a fresh membership and generation check.

use afd_authoring::{Access, Drafts, Observation};
use afd_core::id::Uuid7;
use axum::response::sse::Event;
use futures_util::{Stream, stream};
use std::{collections::HashMap, convert::Infallible, time::Duration};

const QUIET: Duration = Duration::from_secs(1);
const CHECK_DEADLINE: Duration = Duration::from_secs(10);
const FAILED: &str = "draft_stream_recheck_failed";
// A text piece can expand sixfold when escaped, before JSON frame metadata.
const MAX_FRAME_BYTES: usize = 32 * 1024;

struct Watch {
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: i64,
    seen: HashMap<String, String>,
    delay: Duration,
    done: bool,
    subscription: Option<afd_dragonfly::Subscription>,
    _slot: afd_sse::Slot,
}

pub(super) fn frames(
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: i64,
    slot: afd_sse::Slot,
    subscription: Option<afd_dragonfly::Subscription>,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    stream::unfold(
        Watch {
            drafts,
            workspace,
            subject,
            draft,
            generation,
            seen: HashMap::new(),
            delay: Duration::ZERO,
            done: false,
            subscription,
            _slot: slot,
        },
        step,
    )
}

async fn step(mut watch: Watch) -> Option<(Result<Event, Infallible>, Watch)> {
    let heartbeat_at = tokio::time::Instant::now() + afd_sse::HEARTBEAT_INTERVAL;
    while !watch.done {
        let pending = receive(&mut watch).await;
        let access = Access {
            workspace: &watch.workspace,
            subject: &watch.subject,
        };
        let observed = tokio::time::timeout(
            CHECK_DEADLINE,
            watch.drafts.observe(access, &watch.draft, watch.generation),
        )
        .await;
        match observed {
            Ok(Ok(Observation::Deleted)) => {
                watch.done = true;
                return Some((Ok(deleted(&watch.draft)?), watch));
            }
            Ok(Ok(Observation::Current(view))) => {
                watch.generation = view.generation;
                if let Some(pending) = pending
                    && let Some(event) = preview(&watch.draft, &view, pending)
                {
                    watch.delay = QUIET;
                    return Some((Ok(event), watch));
                }
                if let Some(event) = changed(&mut watch, view.notices) {
                    return Some((Ok(event), watch));
                }
                if view.closed {
                    return None;
                }
                watch.delay = QUIET;
                if tokio::time::Instant::now() >= heartbeat_at {
                    let heartbeat = Event::default()
                        .event(afd_sse::HEARTBEAT_EVENT)
                        .data(afd_sse::HEARTBEAT_DATA);
                    return Some((Ok(heartbeat), watch));
                }
            }
            Ok(Err(error)) => {
                let error_code = error.code().as_str();
                tracing::info!(event = FAILED, error_code, "private stream access ended");
                return None;
            }
            Err(_) => return None,
        }
    }
    None
}

fn deleted(draft: &Uuid7) -> Option<Event> {
    Event::default()
        .event("draft.deleted")
        .json_data(serde_json::json!({"draft_id": draft}))
        .ok()
}

/// All paths return to the authorization read before they emit anything.
async fn receive(watch: &mut Watch) -> Option<afd_dragonfly::hub::Received> {
    if watch.delay.is_zero() {
        return None;
    }
    let Some(subscription) = watch.subscription.as_mut() else {
        tokio::time::sleep(watch.delay).await;
        return None;
    };
    tokio::select! {
        () = tokio::time::sleep(watch.delay) => None,
        received = subscription.recv() => if let Ok(received) = received {
            Some(received)
        } else {
            watch.subscription = None;
            None
        },
    }
}

#[derive(garde::Validate)]
struct Payload<'a> {
    #[garde(length(bytes, max = MAX_FRAME_BYTES))]
    value: &'a str,
}

fn preview(
    draft: &Uuid7,
    view: &afd_authoring::View,
    received: afd_dragonfly::hub::Received,
) -> Option<Event> {
    use afd_dragonfly::hub::Received;
    match received {
        Received::Gap | Received::Lagged(_) => {
            Some(Event::default().event("draft.resync").data("{}"))
        }
        Received::Message(message) => {
            let bounded = garde::Unvalidated::new(Payload {
                value: &message.payload,
            })
            .validate()
            .ok()?;
            if let Ok(frame) = serde_json::from_str::<afd_authoring::ContextNotice>(bounded.value) {
                let frame = garde::Unvalidated::new(frame).validate().ok()?;
                if !permitted(draft, view, &frame.draft_id, &frame.sequence) {
                    return None;
                }
                return Event::default()
                    .event(afd_authoring::CONTEXT_EVENT)
                    .json_data(&*frame)
                    .ok();
            }
            let frame: afd_authoring::Delta = serde_json::from_str(bounded.value).ok()?;
            let frame = garde::Unvalidated::new(frame).validate().ok()?;
            if !permitted(draft, view, &frame.draft_id, &frame.sequence) {
                return None;
            }
            Event::default()
                .event(afd_authoring::DELTA_EVENT)
                .json_data(&*frame)
                .ok()
        }
    }
}

fn permitted(draft: &Uuid7, view: &afd_authoring::View, subject: &Uuid7, sequence: &str) -> bool {
    sequence.parse::<i64>().is_ok_and(|sequence| {
        subject == draft
            && !view.closed
            && sequence > view.cancelled_through_sequence
            && view.running_sequence == Some(sequence)
    })
}

fn changed(watch: &mut Watch, notices: Vec<afd_authoring::Notice>) -> Option<Event> {
    let live: std::collections::HashSet<_> = notices.iter().map(key).collect();
    watch.seen.retain(|key, _| live.contains(key));
    for notice in notices {
        let key = key(&notice);
        let body = serde_json::to_string(&notice).ok()?;
        if watch.seen.get(&key) != Some(&body) {
            watch.seen.insert(key, body.clone());
            watch.delay = Duration::ZERO;
            return Some(Event::default().event(notice.event).data(body));
        }
    }
    None
}

fn key(notice: &afd_authoring::Notice) -> String {
    format!(
        "{}:{}",
        notice.event,
        notice.sequence.as_deref().unwrap_or_default()
    )
}
