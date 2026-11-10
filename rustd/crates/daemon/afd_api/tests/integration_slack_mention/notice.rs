//! A mention no fleet can take is owed a notice.
//!
//! Split from the route cases beside it. The router holds a live fleet queue,
//! and existing subscribers carry notices without a separate fleet install.
//! What is read back is the obligation ledger.

#![cfg(feature = "test-util")]

use super::*;

/// One owed notice, as the ledger holds it.
#[derive(Debug)]
pub(super) struct Owed {
    pub(super) owner: String,
    provider: String,
    destination: Option<String>,
    pub(super) answer: String,
}

/// Every obligation the fixture's workspace owes, with the name of the fleet
/// owing it.
pub(super) async fn owed(fixture: &Fixture) -> Vec<Owed> {
    let mut connection = fixture.database().acquire().await.expect("a connection");
    sqlx::query_as::<_, (String, String, Option<String>, String)>(
        "SELECT f.name, o.provider, o.destination, o.answer \
         FROM fleet.obligations o JOIN fleet.fleets f ON f.id = o.fleet_id \
         WHERE o.workspace_id = $1",
    )
    .bind(&fixture.workspace)
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
/// notice naming both, by the first subscriber, in the thread it was asked
/// in, and no event is admitted. Slack's retry of it owes it again: notices
/// carry no dedupe key (Indy, Nov 08, 2026).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retried_notice_is_owed_again() {
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
    let router = fixture.router();
    let body = mention(
        &fixture.team,
        "EvNotice01",
        PERSON,
        &format!("<@{BOT_USER}> anyone around?"),
    );

    let first = json_body(deliver(&router, &body).await).await;
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
        Some(false),
        "{retried}"
    );

    let owed = owed(&fixture).await;
    assert_eq!(owed.len(), 2, "each delivery owes its own notice: {owed:?}");
    let notice = owed.first().expect("the notice is owed");
    assert_eq!(
        notice.owner, "responder",
        "first subscriber owns the notice"
    );
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
