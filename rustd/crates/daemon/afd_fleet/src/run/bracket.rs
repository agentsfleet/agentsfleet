//! The brackets the daemon puts around a run on the live tail.
//!
//! [`super::activity`] forwards the runner's mid-run frames; this module
//! publishes the two the daemon owns — `turn_received` when the run verb
//! opens the narrative log, `turn_complete` when a report or a gate refusal
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
//! contract is [`afd_bus::Handle::publish_frame`]'s, stated once for
//! every daemon-authored frame.
//!
//! # The completion is the row, not a pointer to it
//!
//! `turn_complete` carries the terminal row as the events list would serve
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

use af_core::error_code;
use af_wire::turn::{FleetSteerBody, STEER_MESSAGE_MAX_BYTES, SteerRequest};
use afd_api_wire::tail::{FleetCounters, TailFrame, TailRow};
use afd_turns::view::{Closed, is_fleet_actor, is_steer_actor};
use jiff::Timestamp;

use crate::run::envelope::Acquired;
use crate::run::store::Runs;

/// The tail batch the bus admits is the bound, so a reply this inlines always
/// fits the frame the broker accepts (`afd_bus::nats::preflight`).
const MAX_INLINE_FINAL_REPLY_BYTES: usize = afd_bus::TAIL_BATCH_BYTES;

/// The event a steer body this daemon cannot show is logged under.
const EVENT_STEER_BODY_UNREADABLE: &str = "steer_body_unreadable";

fn inline_final_reply(reply: Option<&str>) -> Option<Cow<'_, str>> {
    reply
        .filter(|text| text.len() <= MAX_INLINE_FINAL_REPLY_BYTES)
        .map(Cow::Borrowed)
}

/// What a person typed, for the received frame: a steer's `message`, within
/// the bound the route admitted it under, a fleet's steer's `message`, or
/// `None`.
///
/// Every other producer's body is its own shape and names no typed words, so
/// only a `steer:` or a `fleet:` actor is read. A fleet's `message` is the
/// sender line and the words, composed and stored by the daemon, so it carries
/// no person's bound. A body that does not parse, or a person's that holds
/// more than the bound, was never written by the route as it stands; it is
/// warned once, without its text, and the frame goes out without a message.
fn steer_message(acquired: &Acquired) -> Option<Cow<'_, str>> {
    if is_fleet_actor(&acquired.actor) {
        return serde_json::from_str::<FleetSteerBody<'_>>(&acquired.request_json)
            .ok()
            .map(|body| body.message)
            .or_else(|| unreadable(acquired));
    }
    if !is_steer_actor(&acquired.actor) {
        return None;
    }
    serde_json::from_str::<SteerRequest<'_>>(&acquired.request_json)
        .ok()
        .map(|request| request.message)
        .filter(|message| message.len() <= STEER_MESSAGE_MAX_BYTES)
        .or_else(|| unreadable(acquired))
}

/// Warns once, without the text, that a steer body could not be shown.
fn unreadable(acquired: &Acquired) -> Option<Cow<'static, str>> {
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let turn_id = acquired.turn_id.as_str();
    {
        let (error_code, event) = (&code, &EVENT_STEER_BODY_UNREADABLE);
        tracing::warn!(error_code, fleet_id, turn_id, event);
    };
    None
}

impl Runs {
    /// One best-effort announcement after a replacement issue commits.
    pub(crate) async fn publish_restarted(&self, acquired: &Acquired, issued: &crate::run::Issued) {
        let frame = TailFrame::RunRestarted {
            turn_id: Cow::Borrowed(acquired.turn_id.as_str()),
            fencing_token: Cow::Owned(acquired.fence.as_i64().to_string()),
            restarted_at: issued.created_at,
        };
        let subject = afd_bus::Subject::ThreadItems(acquired.fleet_id.clone());
        let publication = self.streams().publish_frame(&subject, &frame).await;
        afd_bus::observe_notification(&subject, publication);
    }

    /// Announce that `acquired`'s narrative log opened, stamped `opened_at`:
    /// the row's own `created_at`, which for a steer is its admission instant.
    ///
    /// Published once, on the delivery that wrote the row: a redelivery finds
    /// the row already there and a second announcement would put a duplicate
    /// marker on a tail whose row the client already holds.
    pub async fn publish_received(
        &self,
        acquired: &Acquired,
        opened_at: Timestamp,
        counters: Option<FleetCounters>,
    ) {
        let frame = TailFrame::TurnReceived {
            turn_id: Cow::Borrowed(acquired.turn_id.as_str()),
            actor: Cow::Borrowed(&acquired.actor),
            event_type: Cow::Borrowed(&acquired.event_type),
            created_at: opened_at,
            message: steer_message(acquired),
            counters,
        };
        let subject = afd_bus::Subject::ThreadItems(acquired.fleet_id.clone());
        let publication = self.streams().publish_frame(&subject, &frame).await;
        afd_bus::observe_notification(&subject, publication);
    }

    /// Announce that a run ended, carrying the row the ending wrote. Answers
    /// the original publication failure so the caller decides how to observe it.
    ///
    /// # Errors
    /// Retains serialization and broker failures after the durable report committed.
    pub async fn publish_completion(
        &self,
        closed: &Closed,
        final_reply: Option<&str>,
    ) -> afd_bus::Result<()> {
        let frame = TailFrame::TurnComplete {
            event: Box::new(TailRow::from(closed.row.summary())),
            final_reply: inline_final_reply(final_reply),
            fleet_status: Cow::Borrowed(&closed.fleet_status),
            pending_approvals: closed.pending_approvals,
            counters: Some(closed.counters),
        };
        self.streams()
            .publish_frame(
                &afd_bus::Subject::ThreadItems(closed.row.fleet_id.clone()),
                &frame,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use af_core::test_util::trace::Capture;
    use af_wire::turn::STEER_MESSAGE_MAX_BYTES;

    use super::{
        EVENT_STEER_BODY_UNREADABLE, MAX_INLINE_FINAL_REPLY_BYTES, inline_final_reply,
        steer_message,
    };
    use crate::run::envelope::Acquired;
    use crate::run::test_dead;

    /// A held event raised by `actor` with the body `request_json`.
    fn held(actor: &str, request_json: &str) -> Acquired {
        Acquired {
            predecessor_run_id: None,
            actor: actor.to_owned(),
            request_json: request_json.to_owned(),
            ..test_dead::acquired()
        }
    }

    #[test]
    fn test_event_received_carries_steer_message() {
        let steer = held("steer:user_1", r#"{"message":"check the tests"}"#);
        assert_eq!(steer_message(&steer).as_deref(), Some("check the tests"));
        let webhook = held("webhook:github", r#"{"message":"not typed by a person"}"#);
        assert_eq!(
            steer_message(&webhook),
            None,
            "only a steer names typed words"
        );
        let continuation = held("continuation:steer:user_1", r#"{"message":"x"}"#);
        assert_eq!(
            steer_message(&continuation),
            None,
            "a continuation is not a new message"
        );
    }

    /// A fleet's steer shows its composed message: the sender line, then the
    /// words, with none of a person's bound. A body that does not parse is
    /// warned once, without its text, and shows nothing.
    #[test]
    fn a_fleet_steer_shows_its_sender_line_and_has_no_person_bound() {
        use af_wire::turn::FleetSteerBody;

        let log = Capture::install();
        let sender = "0190d5c2-7e1a-7000-8000-000000000002";
        let event = "0190d5c2-7e1a-7000-8000-000000000004";
        let actor = format!("fleet:{sender}");
        let compose = |text: &str| {
            let body = FleetSteerBody::new(text, sender, "agentmay", event);
            serde_json::to_string(&body).unwrap_or_default()
        };

        let short = held(&actor, &compose("run the check"));
        let brief = steer_message(&short).unwrap_or_default();
        assert!(brief.ends_with("\nrun the check"), "{brief}");
        assert!(brief.starts_with("[from fleet agentmay"), "{brief}");

        let words = "w".repeat(STEER_MESSAGE_MAX_BYTES);
        let heavy = held(&actor, &compose(&words));
        let full = steer_message(&heavy).unwrap_or_default();
        assert!(full.len() > STEER_MESSAGE_MAX_BYTES, "{}", full.len());

        assert_eq!(steer_message(&held(&actor, "{")), None);
        let line = log.only(EVENT_STEER_BODY_UNREADABLE).fields;
        assert!(
            line.values().all(|value| !value.contains('{')),
            "the unreadable body never reaches the log: {line:?}"
        );
    }

    #[test]
    fn test_event_received_without_parsable_body() {
        let log = Capture::install();
        let broken = held("steer:user_1", "{");
        assert_eq!(steer_message(&broken), None);
        let line = log.only(EVENT_STEER_BODY_UNREADABLE).fields;
        assert_eq!(
            line.get("turn_id").map(String::as_str),
            Some(broken.turn_id.as_str())
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
            steer_message(&held("steer:user_1", &body)).map(|text| text.len()),
            Some(STEER_MESSAGE_MAX_BYTES)
        );
        let over = format!("{at_limit}a");
        let body = serde_json::json!({ "message": over }).to_string();
        assert_eq!(steer_message(&held("steer:user_1", &body)), None);
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
