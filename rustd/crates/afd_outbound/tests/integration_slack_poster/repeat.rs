//! A repeat attempt looks in the thread before it posts, so an answer Slack
//! took whose acknowledgement was lost is not posted a second time.
//!
//! Split from the poster's first-attempt cases so each file keeps one
//! question.

use afd_connector::slack::AnswerMarker;

use super::*;

/// The marker the fixture job's answer is posted with.
fn marker(fixture: &Fixture) -> AnswerMarker {
    let job = fixture.job();
    AnswerMarker {
        fleet_id: job.fleet_id,
        event_id: job.event_id,
    }
}

/// The thread as Slack shows it once the answer landed: the question, then
/// the bot's reply carrying `marker`.
fn thread_holding(marker: &AnswerMarker) -> String {
    let stamp = serde_json::to_string(&marker.metadata()).expect("a stamp serializes");
    format!(
        r#"{{"ok":true,"messages":[{{"ts":"{THREAD}","user":"U01","text":"why?"}},
            {{"ts":"1712345678.000900","user":"{BOT_USER}","bot_id":"B01","text":"{ANSWER}","metadata":{stamp}}}]}}"#
    )
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_first_attempt_posts_its_marker_and_reads_nothing() {
    // The common path pays nothing for the check: a first attempt posts
    // straight away, and the post carries what a later repeat looks for.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let verdict = fixture
        .poster(&slack.api_base())
        .deliver(&fixture.job())
        .await;
    assert_eq!(verdict, Verdict::Delivered);

    let requests = slack.requests();
    assert!(
        requests.len() == 1 && requests.iter().all(Request::is_post),
        "one post and no thread read: {requests:?}"
    );
    let stamped: Option<AnswerMarker> = requests
        .first()
        .and_then(|post| post.body.get("metadata"))
        .and_then(|metadata| metadata.get("event_payload"))
        .and_then(|payload| serde_json::from_value(payload.clone()).ok());
    assert_eq!(stamped, Some(marker(&fixture)));

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_repeat_that_finds_its_answer_posts_nothing() {
    // Slack took the first post and the acknowledgement was lost. The repeat
    // reads the thread, sees its own marker, and calls the answer delivered
    // without saying it twice.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    slack.answer(THREAD, 200, &thread_holding(&marker(&fixture)));
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(slack.reads(), 1, "the thread was read");
    assert!(
        !slack.requests().iter().any(Request::is_post),
        "an answer already in the thread is not posted again"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_repeat_whose_answer_is_missing_posts_it_once() {
    // The earlier attempt really did fail: the thread holds only the question,
    // so the repeat posts, once.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Delivered);
    let requests = slack.requests();
    assert_eq!(
        requests.iter().map(Request::is_post).collect::<Vec<_>>(),
        [false, true],
        "a read, then one post"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_repeat_that_cannot_read_the_thread_posts_anyway() {
    // A private channel the grant holds no `groups:history` for. Not knowing
    // is not a yes: the answer is owed to a person, so it goes out.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    slack.answer(THREAD, 200, r#"{"ok":false,"error":"missing_scope"}"#);
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Delivered);
    assert!(
        slack.requests().iter().any(Request::is_post),
        "an unreadable thread still gets its answer"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_repeat_for_a_workspace_holding_no_grant_is_permanent_and_asks_nothing() {
    // A repeat needs the same two inputs as a first attempt. Without a grant
    // there is no token to read the thread with or to post with, so the
    // repeat ends where a first attempt would, before any request.
    let fixture = Fixture::create().await;
    fixture.seed().await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Permanent);
    assert!(slack.requests().is_empty(), "nothing was asked of Slack");

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_marker_another_app_posted_does_not_silence_the_answer() {
    // Any app in the channel can post message metadata. This answer's exact
    // marker under an author that is not the grant's bot is not proof the
    // answer landed, so the repeat posts it.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    let forged = thread_holding(&marker(&fixture)).replace(BOT_USER, "U0OTHERAPP");
    slack.answer(THREAD, 200, &forged);
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(
        slack
            .requests()
            .iter()
            .map(Request::is_post)
            .collect::<Vec<_>>(),
        [false, true],
        "a read, then the answer posted"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_repeat_whose_grant_names_no_bot_user_posts_without_looking() {
    // With no bot user recorded, no marker in the thread can be proven this
    // daemon's own, so reading it would buy nothing: the repeat posts.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture.seal_grant_naming_no_bot_user(BOT_TOKEN).await;

    let slack = slack_answering(200, r#"{"ok":true}"#).await;
    slack.answer(THREAD, 200, &thread_holding(&marker(&fixture)));
    let verdict = fixture
        .poster(&slack.api_base())
        .redeliver(&fixture.job())
        .await;

    assert_eq!(verdict, Verdict::Delivered);
    assert_eq!(
        slack
            .requests()
            .iter()
            .map(Request::is_post)
            .collect::<Vec<_>>(),
        [true],
        "one post and no thread read"
    );

    fixture.cleanup().await;
}
