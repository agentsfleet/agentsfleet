//! The one poster this build ships, against the two stores it actually holds.
//!
//! `dispatch` routes exactly one provider — `outbound/worker.zig:124` does the
//! same and says so — and `integration_worker.rs` drives it through a stub
//! `Deliver`, because what that suite grades is the consumer group and the
//! retry loop. So `SlackPoster` itself ran no covered lines: the address read
//! that finds where an answer goes, the vault read that opens the bot token,
//! and the POST that carries it were all unproven.
//!
//! # Why a loopback Slack rather than a stubbed client
//!
//! The verdict a status maps to is already graded without a server in
//! `delivery.rs` — no server can make a vendor answer 429 three times on
//! demand. What only a socket can show is the REQUEST: that the bearer is the
//! token the vault opened, that the channel and thread come from the address
//! the job carries — no event row exists here to read them from — and that a
//! 200 carrying `{"ok":false}` is not a delivery. A stubbed client would assert
//! the arguments this test passed it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_connector::test_util::{FakeSlack, Request};
use afd_outbound::{Deliver as _, Verdict};

#[path = "integration_slack_poster/fixture.rs"]
mod fixture;

use self::fixture::Fixture;

/// The bot token the vault holds for this workspace.
const BOT_TOKEN: &str = "xoxb-fixture-bot-token";

/// Where the answer is threaded, as a Slack producer records it.
const CHANNEL: &str = "C0FIXTURE01";
/// See [`CHANNEL`].
const THREAD: &str = "1712345678.000100";

/// What the fleet is answering with.
const ANSWER: &str = "the fixture answer";

/// A Slack base nothing listens on: any request sent here fails in transport.
const NOBODY_LISTENING: &str = "http://127.0.0.1:1";

/// A loopback Slack answering `status` with `body` to every post.
async fn slack_answering(status: u16, body: &str) -> FakeSlack {
    let slack = FakeSlack::start().await;
    slack.post_answers(status, body);
    slack
}

/// The post the poster sent, which it has finished sending by the time a
/// verdict came back. Fails the case when nothing was posted, so a regression
/// answering `Delivered` without dialling cannot pass.
fn received(slack: &FakeSlack) -> Request {
    slack
        .requests()
        .into_iter()
        .next()
        .expect("the poster posted to the fake Slack")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn poster_posts_to_the_jobs_address() {
    // The whole read path in one pass, and every field asserted is one the
    // daemon looked up rather than one this test handed it: the bearer is the
    // token the vault opened, and the channel and thread are the address the
    // job carries.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let verdict = fixture.poster(slack.base()).deliver(&fixture.job()).await;
    assert_eq!(
        verdict,
        Verdict::Delivered,
        "a Slack that accepted is a delivery"
    );

    let sent = received(&slack);
    assert_eq!(
        sent.authorization,
        format!("Bearer {BOT_TOKEN}"),
        "the post must carry the token the vault opened"
    );
    assert_eq!(
        (sent.field("channel"), sent.field("thread_ts")),
        (Some(CHANNEL), Some(THREAD)),
        "the answer must be threaded under the message that asked it, from the \
         job's own address rather than from anywhere else"
    );
    assert_eq!(
        sent.field("text"),
        Some(ANSWER),
        "the answer itself must be sent"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_holding_no_grant_is_permanent_rather_than_retried() {
    // Uninstalled, disconnected, or a grant that landed malformed. Reconnecting
    // is the only fix, so a retry budget spent here is a queue head blocked on
    // an answer that can never go out.
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let verdict = fixture.poster(slack.base()).deliver(&fixture.job()).await;
    assert_eq!(verdict, Verdict::Permanent);

    drop(slack);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unreadable_address_is_permanent_without_a_request() {
    // Missing a field, empty, or not JSON — one answer for all three, because a
    // caller does the same thing with each: the job names nowhere to post, and
    // no retry changes that. The poster is pointed at a port nothing listens
    // on, where any request it built would fail in transport and answer
    // `Retryable`; `Permanent` is therefore proof that none was attempted.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    for address in [
        format!(r#"{{"channel_id":"{CHANNEL}"}}"#),
        format!(r#"{{"channel_id":"{CHANNEL}","thread_ts":""}}"#),
        "not json".to_owned(),
    ] {
        let mut nowhere = fixture.job();
        nowhere.destination = address;
        assert_eq!(
            fixture.poster(NOBODY_LISTENING).deliver(&nowhere).await,
            Verdict::Permanent,
            "`{}` names nowhere, and nothing was asked of Slack",
            nowhere.destination
        );
    }

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_two_hundred_that_says_not_ok_is_not_a_delivery() {
    // Slack answers its own failures with 200 and `ok: false`. Reading the
    // status alone would acknowledge an answer that never reached the channel,
    // and the person who asked would be waiting for something already dropped.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":false,"error":"channel_not_found"}"#).await;
    let verdict = fixture.poster(slack.base()).deliver(&fixture.job()).await;
    assert_ne!(
        verdict,
        Verdict::Delivered,
        "a 200 carrying `ok: false` is Slack refusing, not accepting"
    );

    let _sent = received(&slack);
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_vendor_that_is_briefly_unwell_is_retried() {
    // A 5xx is the one class worth spending the budget on, and this is the case
    // that separates it from the two permanent ones above.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(503, r#"{"ok":false}"#).await;
    let verdict = fixture.poster(slack.base()).deliver(&fixture.job()).await;
    assert_eq!(verdict, Verdict::Retryable);

    let _sent = received(&slack);
    fixture.cleanup().await;
}
