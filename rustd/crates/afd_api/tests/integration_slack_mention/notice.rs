//! A mention no fleet can take is owed a notice, once.
//!
//! Split from the route cases beside it. The router holds a live fleet queue,
//! because the notice's owner is the channel's resident and the first notice in
//! a channel installs it. What is read back is the obligation ledger.

#![cfg(feature = "test-util")]

use afd_ingress::slack::notice_key;

use super::*;

/// One owed notice, as the ledger holds it.
#[derive(Debug)]
pub(super) struct Owed {
    pub(super) owner: String,
    provider: String,
    destination: Option<String>,
    pub(super) answer: String,
}

/// Every obligation owed under `event_id`, with the name of the fleet owing it.
pub(super) async fn owed(fixture: &Fixture, event_id: &str) -> Vec<Owed> {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_as::<_, (String, String, Option<String>, String)>(
        "SELECT f.name, o.provider, o.destination, o.answer \
         FROM core.fleet_obligations o JOIN core.fleets f ON f.id = o.fleet_id \
         WHERE o.event_id = $1",
    )
    .bind(event_id)
    .fetch_all(&mut *connection)
    .await
    .expect("the ledger reads")
    .into_iter()
    .map(|(owner, provider, destination, answer)| Owed {
        owner,
        provider,
        destination,
        answer,
    })
    .collect()
}

/// Dimension 6.2 — an unaddressed mention two fleets could answer is owed one
/// notice naming both, by the channel's resident, in the thread it was asked
/// in; Slack's retry of it owes nothing more, and no event is admitted.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn retried_notice_is_owed_once() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    for fleet in ["responder", "triage"] {
        fixture
            .fleet(
                &document(fleet, CHANNEL, Some("read")),
                FleetStatus::Active.as_str(),
            )
            .await;
    }
    let router = fixture.resident_router().await;
    let body = mention(
        &fixture.team,
        "EvNotice01",
        PERSON,
        &format!("<@{BOT_USER}> anyone around?"),
    );
    let key = notice_key(&fixture.team, "EvNotice01");

    let first = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        first.get("event_id").and_then(Value::as_str),
        Some(key.as_str())
    );
    assert_eq!(first.get("replayed").and_then(Value::as_bool), Some(false));
    let at = harness::frozen_unix_seconds().to_string();
    let retried = json_body(
        deliver_at(
            &router,
            SIGNING_SECRET,
            &at,
            &body,
            &[(name(RETRY_HEADER), "1")],
        )
        .await,
    )
    .await;
    assert_eq!(
        retried.get("replayed").and_then(Value::as_bool),
        Some(true),
        "{retried}"
    );

    let owed = owed(&fixture, &key).await;
    assert_eq!(owed.len(), 1, "one notice for one Slack event: {owed:?}");
    let notice = owed.first().expect("the notice is owed");
    assert!(notice.owner.starts_with("slack-channel-"), "{notice:?}");
    assert_eq!(notice.provider, PROVIDER.id());
    let thread = notice
        .destination
        .as_deref()
        .and_then(Thread::parse)
        .expect("the notice is addressed to a thread");
    assert_eq!(
        (thread.channel_id.as_str(), thread.thread_ts.as_str()),
        (CHANNEL, THREAD_TS)
    );
    assert!(
        notice.answer.contains("responder") && notice.answer.contains("triage"),
        "the notice names the fleets to choose from: {}",
        notice.answer
    );
    assert_eq!(fixture.admissions().await, 0, "a notice admits no event");

    fixture.cleanup().await;
}
