//! What a steer's body is allowed to be, and how a failed steer is answered.
//!
//! The write decides whether the bytes a client sent are a message at all
//! before any datastore is reached, so it is proven here;
//! `fleet_messages_steer.rs` proves the refusal survives the layer stack.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_observability::metrics::label::fleet::SteerPlaced;
use af_wire::event::STEER_MESSAGE_MAX_BYTES;
use afd_events::disposition::{Placed, SteerDisposition};
use axum::body::Bytes;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde_json::{Value, json};

use af_core::id::Uuid7;
use afd_auth::principal::{Person, PersonCredential, Subject};
use afd_auth::scope::ScopeSet;

use super::{accepted, actor_for, counted, read_steer, refuse_steer};

const TENANT: &str = "01924f4e-0000-7000-8000-00000000a0a1";
const EVENT: &str = "0190d5c2-7e1a-7000-8000-00000000e002";
const RUN: &str = "0190d5c2-7e1a-7000-8000-00000000e001";
const HUMAN: &str = "user_2steerer";

fn person(credential: PersonCredential) -> Person {
    Person::new(
        credential,
        Uuid7::parse(TENANT).expect("a canonical tenant id"),
        Subject::new(HUMAN).expect("a non-blank subject"),
        ScopeSet::EMPTY,
    )
}

/// A steer from a browser session or a terminal credential names its human; a
/// tenant api-key names the machine, because a key is automation even though
/// a person created it.
#[test]
fn a_steer_is_attributed_to_a_person_only_when_one_sent_it() {
    let human = afd_events::steer_actor(HUMAN);
    let session = person(PersonCredential::SessionToken {
        workspace_scope: None,
    });
    assert_eq!(actor_for(&session), human);
    assert_eq!(actor_for(&person(PersonCredential::CliCredential)), human);
    assert_eq!(
        actor_for(&person(PersonCredential::TenantApiKey)),
        afd_events::ACTOR_MACHINE
    );
}

/// Only a reused operation id is answered as a 409 naming the id's state; every
/// other steer failure keeps the status its plane decided.
///
/// The claim is that the two arms stay apart: a datastore outage rendered as
/// "admitted" would tell a caller to stop retrying a message that never landed.
#[test]
fn only_a_reused_id_is_answered_as_admitted() {
    for (label, error) in afd_events::error::one_of_each_kind() {
        let conflict = error.is_operation_conflict();
        let status = refuse_steer(error).into_response().status();
        assert_eq!(
            status == StatusCode::CONFLICT,
            conflict,
            "{label} answered {status}"
        );
    }
}

/// A steer with nothing in it is refused before the parser runs.
#[test]
fn should_refuse_a_steer_that_carries_no_body() {
    read_steer(&Bytes::new()).unwrap_err();
}

/// A body this daemon cannot read is refused.
#[test]
fn should_refuse_a_body_that_is_not_a_message() {
    for body in [
        "",
        "{",
        "null",
        "[]",
        r#""hello""#,
        "{}",
        r#"{"message":null}"#,
        r#"{"message":7}"#,
    ] {
        assert!(
            read_steer(&Bytes::from(body.as_bytes().to_vec())).is_err(),
            "{body} is not a steer this surface accepts"
        );
    }
}

/// An empty message is refused: a person pressed send on nothing.
#[test]
fn should_refuse_an_empty_message() {
    read_steer(&Bytes::from_static(br#"{"message":""}"#)).unwrap_err();
}

/// An escaped message is a message, not a malformed body.
///
/// The regression this file exists for on the write side. `serde` hands back
/// `Cow::Owned` for any string carrying an escape, so a reader that accepted
/// only a borrow would refuse a newline, a quote and an emoji — which is most
/// of what a person types into a chat box.
#[test]
fn should_read_a_message_that_carries_escapes() {
    let body = Bytes::from_static(br#"{"message":"line one\nline \"two\"\tand \u2728 done"}"#);
    assert_eq!(
        read_steer(&body).unwrap().message,
        "line one\nline \"two\"\tand \u{2728} done",
    );
}

/// The length bound is on the DECODED bytes, not on what a client sent.
///
/// A message of newlines doubles in the encoded form: bounding the escaped
/// bytes would refuse a message half the documented size, and the runner reads
/// the decoded text.
#[test]
fn should_bound_the_decoded_bytes_and_not_the_escaped_ones() {
    let escaped = format!(
        r#"{{"message":"{}"}}"#,
        r"\n".repeat(STEER_MESSAGE_MAX_BYTES)
    );
    let body = Bytes::from(escaped.into_bytes());
    let read = read_steer(&body).expect("a message of newlines is under the bound once decoded");
    assert_eq!(read.message.len(), STEER_MESSAGE_MAX_BYTES);
}

/// The bound admits its own ceiling and refuses one byte past it.
#[test]
fn should_admit_the_ceiling_and_refuse_one_byte_past_it() {
    let at_the_ceiling = format!(r#"{{"message":"{}"}}"#, "a".repeat(STEER_MESSAGE_MAX_BYTES));
    read_steer(&Bytes::from(at_the_ceiling.into_bytes()))
        .expect("the documented ceiling is a message this surface takes");

    let one_past = format!(
        r#"{{"message":"{}"}}"#,
        "a".repeat(STEER_MESSAGE_MAX_BYTES + 1)
    );
    read_steer(&Bytes::from(one_past.into_bytes())).unwrap_err();
}

/// A fleet that takes no work answers 409 `UZ-AGT-012` naming its status.
#[tokio::test]
async fn a_stopped_fleet_refusal_names_its_status() {
    let response = super::not_runnable(afd_fleet_lifecycle::FleetStatus::Paused).into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the refusal body");
    let rendered = String::from_utf8(body.to_vec()).expect("a JSON body");
    assert!(rendered.contains("UZ-AGT-012"), "{rendered}");
    assert!(
        rendered.contains(r#""current_state":"paused""#),
        "{rendered}"
    );
}

/// The 202 body of an accepted steer, read back as the client reads it.
async fn accepted_body(
    event_id: &str,
    replayed: bool,
    disposition: SteerDisposition,
    joined: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let placed = Placed {
        disposition,
        folded_into_event_id: joined.map(str::to_owned),
    };
    let response: Response = accepted(event_id.to_owned(), replayed, placed);
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// The 202 names where the message went, and the run a fold joined; a send
/// that joins no run carries no `folded_into_event_id` at all.
#[tokio::test]
async fn the_accepted_body_names_where_the_message_went() -> Result<(), Box<dyn std::error::Error>>
{
    assert_eq!(
        accepted_body(EVENT, false, SteerDisposition::Folded, Some(RUN)).await?,
        json!({
            "status": "accepted",
            "event_id": EVENT,
            "replayed": false,
            "disposition": "folded",
            "folded_into_event_id": RUN,
        })
    );
    for (disposition, word) in [
        (SteerDisposition::Started, "started"),
        (SteerDisposition::Queued, "queued"),
    ] {
        assert_eq!(
            accepted_body(EVENT, false, disposition, None).await?,
            json!({
                "status": "accepted",
                "event_id": EVENT,
                "replayed": false,
                "disposition": word,
            })
        );
    }
    Ok(())
}

/// A repeat's 202 says it is one, and still names where the message sits.
#[tokio::test]
async fn a_repeats_accepted_body_says_so() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        accepted_body(EVENT, true, SteerDisposition::Folded, Some(RUN)).await?,
        json!({
            "status": "accepted",
            "event_id": EVENT,
            "replayed": true,
            "disposition": "folded",
            "folded_into_event_id": RUN,
        })
    );
    Ok(())
}

/// A message is counted once, where this call placed it; a repeat of it is
/// answered without a second tick.
#[test]
fn only_a_newly_admitted_message_is_counted() {
    for (disposition, placed) in [
        (SteerDisposition::Started, SteerPlaced::Started),
        (SteerDisposition::Folded, SteerPlaced::Folded),
        (SteerDisposition::Queued, SteerPlaced::Queued),
    ] {
        assert_eq!(counted(false, disposition), Some(placed));
        assert_eq!(counted(true, disposition), None, "{disposition:?}");
    }
}
