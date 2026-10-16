//! No buffered private frame bypasses a fresh membership and generation check.

use afd_authoring::{Access, Drafts, Observation};
use afd_core::id::Uuid7;
use axum::response::sse::Event;
use futures_util::{Stream, stream};
use std::{collections::HashMap, convert::Infallible, time::Duration};

const QUIET: Duration = Duration::from_secs(1);
const CHECK_DEADLINE: Duration = Duration::from_secs(10);
const FAILED: &str = "draft_stream_recheck_failed";

struct Watch {
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: Option<i64>,
    seen: HashMap<String, String>,
    delay: Duration,
    done: bool,
    _slot: afd_sse::Slot,
}

pub(super) fn frames(
    drafts: Drafts,
    workspace: Uuid7,
    subject: String,
    draft: Uuid7,
    generation: i64,
    slot: afd_sse::Slot,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    stream::unfold(
        Watch {
            drafts,
            workspace,
            subject,
            draft,
            generation: Some(generation),
            seen: HashMap::new(),
            delay: Duration::ZERO,
            done: false,
            _slot: slot,
        },
        step,
    )
}

async fn step(mut watch: Watch) -> Option<(Result<Event, Infallible>, Watch)> {
    while !watch.done {
        tokio::time::sleep(watch.delay).await;
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
                let event = Event::default()
                    .event("draft.deleted")
                    .json_data(serde_json::json!({"draft_id":watch.draft}))
                    .ok()?;
                return Some((Ok(event), watch));
            }
            Ok(Ok(Observation::Current(view))) => {
                watch.generation = Some(view.generation);
                if let Some(event) = changed(&mut watch, view.notices) {
                    return Some((Ok(event), watch));
                }
                if view.closed {
                    return None;
                }
                watch.delay = QUIET;
            }
            Ok(Err(error)) => {
                tracing::info!(
                    event = FAILED,
                    error_code = error.code().as_str(),
                    "private stream access ended"
                );
                return None;
            }
            Err(_) => return None,
        }
    }
    None
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
