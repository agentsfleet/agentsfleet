//! The brackets the daemon puts around a run on the live tail.
//!
//! [`super::activity`] forwards the runner's mid-run frames; this module
//! publishes the two the daemon owns — `event_received` when the lease verb
//! opens the narrative log, `event_complete` when a report or a gate refusal
//! closes it. `docs/architecture/runner_fleet.md` §Live activity names them
//! the bracket frames: open and close markers that reach a watcher even for a
//! run the runner never forwarded a single frame from.
//!
//! # Best-effort, like every publish on this channel
//!
//! A frame that does not land costs the tail a marker and the run nothing.
//! The durable row is written first and stands whether or not anybody was
//! listening; the frame is the row's announcement, and a lost announcement is
//! recovered by the client's reconnect backfill from the events list. That
//! contract is [`afd_dragonfly::FleetStreams::publish_frame`]'s, stated once for
//! every daemon-authored frame.
//!
//! # The completion is the row, not a pointer to it
//!
//! `event_complete` carries the terminal row as the events list would serve
//! it, plus the fleet's status, pending gate count, and activity counters, all
//! read by the closing statement in the same round trip that ended the run. A
//! watcher folds the frame in and issues no read — which is why the dashboard's
//! summary strip and the wall's tiles move on a completion without fetching
//! anything.
//!
//! The opening bracket cannot read its counters that way: its own insert is
//! what fires the counter trigger, and a `RETURNING` on that insert does not
//! see the trigger's write. The caller reads them after the row landed and
//! hands them in — `None` when the read did not answer, never zeros.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_events::{ACTOR_PREFIX, Closed};
use afd_wire::event::{STEER_MESSAGE_MAX_BYTES, SteerRequest};
use afd_wire::tail::{FleetCounters, TailFrame, TailRow};

use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

const MAX_INLINE_FINAL_REPLY_BYTES: usize = 64 * 1024;

/// The event a steer body this daemon cannot show is logged under.
const EVENT_STEER_BODY_UNREADABLE: &str = "steer_body_unreadable";

fn inline_final_reply(reply: Option<&str>) -> Option<Cow<'_, str>> {
    reply
        .filter(|text| text.len() <= MAX_INLINE_FINAL_REPLY_BYTES)
        .map(Cow::Borrowed)
}

/// What a person typed, for the received frame: a steer's `message`, within
/// the bound the route admitted it under, or `None`.
///
/// Every other producer's body is its own shape and names no typed words, so
/// only a `steer:` actor is read. A steer body that does not parse, or holds
/// more than the bound, was never written by the route as it stands; it is
/// warned once, without its text, and the frame goes out without a message.
fn steer_message(acquired: &Acquired) -> Option<Cow<'_, str>> {
    if !acquired.actor.starts_with(ACTOR_PREFIX) {
        return None;
    }
    serde_json::from_str::<SteerRequest<'_>>(&acquired.request_json)
        .ok()
        .map(|request| request.message)
        .filter(|message| message.len() <= STEER_MESSAGE_MAX_BYTES)
        .or_else(|| {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let fleet_id = acquired.fleet_id.as_str();
            let event_id = acquired.event_id.as_str();
            tracing::warn!(
                error_code = code,
                fleet_id,
                event_id,
                event = EVENT_STEER_BODY_UNREADABLE,
            );
            None
        })
}

impl Leases {
    /// Announce that `acquired`'s narrative log opened at `now`.
    ///
    /// Published once, on the delivery that wrote the row: a redelivery finds
    /// the row already there and a second announcement would put a duplicate
    /// marker on a tail whose row the client already holds.
    pub async fn publish_received(
        &self,
        acquired: &Acquired,
        now: UnixMillis,
        counters: Option<FleetCounters>,
    ) {
        let frame = TailFrame::EventReceived {
            event_id: Cow::Borrowed(&acquired.event_id),
            actor: Cow::Borrowed(&acquired.actor),
            event_type: Cow::Borrowed(&acquired.event_type),
            created_at: now.as_millis(),
            message: steer_message(acquired),
            counters,
        };
        self.streams()
            .publish_frame(acquired.fleet_id.as_str(), &frame)
            .await;
    }

    /// Announce that a run ended, carrying the row the ending wrote.
    pub async fn publish_completion(&self, closed: &Closed, final_reply: Option<&str>) {
        let frame = TailFrame::EventComplete {
            event: Box::new(TailRow::from(closed.row.summary())),
            final_reply: inline_final_reply(final_reply),
            fleet_status: Cow::Borrowed(&closed.fleet_status),
            pending_approvals: closed.pending_approvals,
            counters: Some(closed.counters),
        };
        self.streams()
            .publish_frame(&closed.row.fleet_id, &frame)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use afd_core::test_util::trace::Capture;
    use afd_wire::event::STEER_MESSAGE_MAX_BYTES;

    use super::{
        EVENT_STEER_BODY_UNREADABLE, MAX_INLINE_FINAL_REPLY_BYTES, inline_final_reply,
        steer_message,
    };
    use crate::lease::envelope::Acquired;
    use crate::lease::test_dead;

    /// A leased event raised by `actor` with the body `request_json`.
    fn leased(actor: &str, request_json: &str) -> Acquired {
        Acquired {
            actor: actor.to_owned(),
            request_json: request_json.to_owned(),
            ..test_dead::acquired()
        }
    }

    #[test]
    fn test_event_received_carries_steer_message() {
        let steer = leased("steer:user_1", r#"{"message":"check the tests"}"#);
        assert_eq!(steer_message(&steer).as_deref(), Some("check the tests"));
        let webhook = leased("webhook:github", r#"{"message":"not typed by a person"}"#);
        assert_eq!(
            steer_message(&webhook),
            None,
            "only a steer names typed words"
        );
        let continuation = leased("continuation:steer:user_1", r#"{"message":"x"}"#);
        assert_eq!(
            steer_message(&continuation),
            None,
            "a continuation is not a new message"
        );
    }

    #[test]
    fn test_event_received_without_parsable_body() {
        let log = Capture::install();
        let broken = leased("steer:user_1", "{");
        assert_eq!(steer_message(&broken), None);
        let line = log.only(EVENT_STEER_BODY_UNREADABLE).fields;
        assert_eq!(
            line.get("event_id").map(String::as_str),
            Some(broken.event_id.as_str())
        );
        assert!(
            line.values().all(|value| !value.contains('{')),
            "the unreadable body never reaches the log: {line:?}"
        );
    }

    /// The bound the route admits under, at its edge: exactly the bound is
    /// shown, one byte over is dropped and warned.
    #[test]
    fn should_bound_a_received_message_at_the_admission_limit() {
        let log = Capture::install();
        let at_limit = "a".repeat(STEER_MESSAGE_MAX_BYTES);
        let body = serde_json::json!({ "message": at_limit }).to_string();
        assert_eq!(
            steer_message(&leased("steer:user_1", &body)).map(|text| text.len()),
            Some(STEER_MESSAGE_MAX_BYTES)
        );
        let over = format!("{at_limit}a");
        let body = serde_json::json!({ "message": over }).to_string();
        assert_eq!(steer_message(&leased("steer:user_1", &body)), None);
        let _warned = log.only(EVENT_STEER_BODY_UNREADABLE);
    }

    #[test]
    fn inline_answer_accepts_empty_and_exact_limit_but_skips_oversized() {
        assert_eq!(inline_final_reply(None), None);
        assert_eq!(inline_final_reply(Some("")), Some("".into()));
        let at_limit = "a".repeat(MAX_INLINE_FINAL_REPLY_BYTES);
        assert_eq!(
            inline_final_reply(Some(&at_limit)).as_deref(),
            Some(at_limit.as_str())
        );
        let over_limit = format!("{at_limit}a");
        assert_eq!(inline_final_reply(Some(&over_limit)), None);
    }
}
