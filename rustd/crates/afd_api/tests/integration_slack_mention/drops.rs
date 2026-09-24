//! What a verified mention never admits, and what it answers instead.
//!
//! Split from the admission cases beside it: those assert one ledger row and
//! its fields, and every case here asserts the opposite — the drop reason or
//! the refusal on the wire, and a ledger with nothing new in it.

#![cfg(feature = "test-util")]

use super::*;

/// The bot's own message mentioning itself is dropped, identified by the user
/// id its grant records, and admits nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_mention_by_the_bot_itself_is_dropped() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();

    let body = mention(
        &fixture.team,
        "Ev03",
        BOT_USER,
        &format!("<@{BOT_USER}> done"),
    );
    let document = json_body(deliver(&router, &body).await).await;
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some("bot_message"),
        "{document}"
    );
    assert_eq!(fixture.admissions().await, 0);

    fixture.cleanup().await;
}

/// Dimension 1.4 — a team this deployment never installed into is dropped
/// with 200, not refused: a refusal would put it in Slack's retry loop.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn unmapped_team_is_dropped_not_refused() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router();

    let body = mention(
        "T0NOTINSTALLED",
        "Ev04",
        PERSON,
        &format!("<@{BOT_USER}> hello"),
    );
    let answered = deliver(&router, &body).await;
    assert_eq!(answered.status(), StatusCode::OK);
    let document = json_body(answered).await;
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some("team_not_mapped"),
        "{document}"
    );

    fixture.cleanup().await;
}

/// Dimension 1.5 — a bad signature and a stale timestamp are refused with 401
/// before the body is parsed, and admit nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn wall_still_refuses_before_parsing() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, None),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();
    let body = mention(
        &fixture.team,
        "Ev05",
        PERSON,
        &format!("<@{BOT_USER}> hello"),
    );
    let now = harness::frozen_unix_seconds();

    let forged = deliver_at(
        &router,
        b"not-the-signing-secret",
        &now.to_string(),
        &body,
        &[],
    )
    .await;
    assert_eq!(
        forged.status(),
        StatusCode::UNAUTHORIZED,
        "a forged mention is refused"
    );
    let stale = deliver_at(
        &router,
        SIGNING_SECRET,
        &now.saturating_sub(3_600).to_string(),
        &body,
        &[],
    )
    .await;
    assert_eq!(
        stale.status(),
        StatusCode::UNAUTHORIZED,
        "a replayed old mention is refused"
    );
    assert_eq!(fixture.admissions().await, 0, "neither reached the ledger");

    fixture.cleanup().await;
}
