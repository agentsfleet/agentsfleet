//! Asking a Slack thread whether an answer is already in it, over a socket.
//!
//! The marker match is unit-tested beside the code. What only a socket shows
//! is the read itself: that it asks Slack for message metadata, follows the
//! cursor until the marker turns up and then stops, and that a thread it
//! cannot read is a reason the poster can post past rather than a yes.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_connector::slack::{self, AnswerMarker, Thread, Unavailable};
use afd_connector::test_util::{FakeSlack, Reply};
use afd_crypto::secret::SecretString;

/// The bot token the grant holds.
const TOKEN: &str = "xoxb-fixture-answered";
/// The bot user the grant recorded: the author a marker must carry.
const BOT_USER: &str = "U0BOTAF01";

fn thread() -> Thread {
    Thread {
        team_id: Some("T024BE7LD".to_owned()),
        channel_id: "C0123456789".to_owned(),
        thread_ts: "1700000000.000100".to_owned(),
    }
}

fn marker() -> AnswerMarker {
    AnswerMarker {
        fleet_id: "0195b4ba-8d3a-7a11-8abc-000000000003".to_owned(),
        event_id: "1760000000001-0".to_owned(),
    }
}

/// One page of a thread: the root, then `reply`, with a cursor when `more`.
fn page(reply: &str, more: bool) -> Reply {
    let cursor = if more { "bmV4dA==" } else { "" };
    Reply::answers(
        200,
        &format!(
            r#"{{"ok":true,"has_more":{more},"response_metadata":{{"next_cursor":"{cursor}"}},
                "messages":[{{"ts":"1700000000.000100","user":"U01","text":"why did it fail?"}},{reply}]}}"#
        ),
    )
}

/// A reply this daemon posted carrying `marker`.
fn answered(marker: &AnswerMarker) -> String {
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    format!(
        r#"{{"ts":"1700000000.000900","user":"{BOT_USER}","bot_id":"B01","text":"the answer","metadata":{stamp}}}"#
    )
}

async fn check(fake: &FakeSlack) -> Result<bool, Unavailable> {
    let token = SecretString::new(TOKEN.to_owned());
    slack::holds_answer(
        &reqwest::Client::new(),
        &fake.api_base(),
        &token,
        &thread(),
        &marker(),
        BOT_USER,
    )
    .await
}

/// The marker on the second page is found, and the read asks Slack for the
/// metadata that carries it on every page.
#[tokio::test]
async fn an_answer_on_a_later_page_is_found() {
    let person = r#"{"ts":"1700000000.000200","user":"U02","text":"looking"}"#;
    let fake =
        FakeSlack::in_order(vec![page(person, true), page(&answered(&marker()), false)]).await;

    assert_eq!(check(&fake).await, Ok(true));
    let requests = fake.requests();
    assert_eq!(requests.len(), 2, "one read per page");
    for request in &requests {
        assert_eq!(request.field("include_all_metadata"), Some("true"));
        assert_eq!(request.authorization, format!("Bearer {TOKEN}"));
        assert_eq!(
            request.field("oldest"),
            Some("1759999700.000000"),
            "every page starts shortly before the question, not at the thread's root"
        );
    }
}

/// The read stops at the page the marker is on; the pages after it are never
/// asked for.
#[tokio::test]
async fn the_read_stops_at_the_marker() {
    let fake = FakeSlack::in_order(vec![page(&answered(&marker()), true)]).await;

    assert_eq!(check(&fake).await, Ok(true));
    assert_eq!(fake.requests().len(), 1, "the cursor was not followed");
}

/// Another answer's marker and a person's reply are a thread that does not
/// hold this answer yet.
#[tokio::test]
async fn a_thread_without_this_answer_holds_nothing() {
    let other = AnswerMarker {
        event_id: "1760000000002-0".to_owned(),
        ..marker()
    };
    let fake = FakeSlack::in_order(vec![page(&answered(&other), false)]).await;

    assert_eq!(check(&fake).await, Ok(false));
}

/// A thread Slack will not show — a private channel the grant cannot read —
/// is a reason, never a yes.
#[tokio::test]
async fn an_unreadable_thread_is_a_reason_not_a_yes() {
    let fake = FakeSlack::in_order(vec![Reply::answers(
        200,
        r#"{"ok":false,"error":"missing_scope"}"#,
    )])
    .await;

    assert_eq!(check(&fake).await, Err(Unavailable::Refused));
}

/// A Slack that never answers is a timeout under the check's own deadline,
/// never a yes, and never a wait past that deadline.
#[tokio::test]
async fn a_thread_that_never_answers_is_a_timeout_not_a_yes() {
    let fake = FakeSlack::in_order(vec![Reply::Stalls]).await;
    let started = std::time::Instant::now();

    assert_eq!(check(&fake).await, Err(Unavailable::Timeout));
    assert!(
        started.elapsed() < slack::ANSWER_CHECK_DEADLINE * 2,
        "bounded by the check's own deadline: {:?}",
        started.elapsed()
    );
}
