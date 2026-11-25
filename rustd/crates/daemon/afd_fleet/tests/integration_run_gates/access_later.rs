//! A revoke and a waiting card, each as the fleet's next run sees them.

use super::access::{Live, SENTRY, answer, holds_sentry, raised};
use super::*;

use af_crypto::entropy::Entropy;
use afd_approval::Decision;
use serde_json::{Value, json};

// Value: protects=a revoke reaches the fleet's next run, and a token mint for
// the revoked name is refused; fails_when=the policy or the mint reads a
// grant once and keeps it; seam=`IntegrationGrants::revoke`, a delivery and
// the mint verb over live Postgres.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_revoked_grant_leaves_the_next_run_without_the_secret() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    let gate = raised(&fixtures, &live).await;
    answer(&fixtures, &live.ready.fleet, &gate, Decision::Approved).await;
    let github = json!({"repositories":["agentsfleet/probe"],"access":"read"});
    fixtures.approve_github(&live.ready.fleet, &github).await;
    let run = live
        .payload
        .get("run_id")
        .and_then(Value::as_str)
        .expect("the live run");
    let before = mint_code(&fixtures, &live, run).await;

    for name in [SENTRY, crate::grants::GITHUB] {
        revoke(&fixtures, &live, name).await;
    }
    let after = mint_code(&fixtures, &live, run).await;
    let next = live
        .next(&fixtures)
        .await
        .expect("the fleet's next head is delivered");

    assert_ne!(
        before,
        Some(af_core::error_code::GRANT_NOT_FOUND),
        "approved, it is minted"
    );
    assert_eq!(after, Some(af_core::error_code::GRANT_NOT_FOUND));
    assert_eq!(holds_sentry(&next), (false, false));
    fixtures.cleanup().await;
}

/// The code a GitHub mint under `run` is refused with, if any.
async fn mint_code(
    fixtures: &Fixtures,
    live: &Live,
    run: &str,
) -> Option<af_core::error_code::ErrorCode> {
    fixtures
        .plane()
        .mint(
            &live.ready.runner,
            &crate::requests::mint(run, crate::grants::GITHUB),
            fixtures.enrolled_at(),
        )
        .await
        .err()
        .map(|refused| refused.code())
}

/// A person's revoke of the fleet's grant of `name`.
async fn revoke(fixtures: &Fixtures, live: &Live, name: &str) {
    let grant: Uuid7 = sqlx::query_scalar(
        "SELECT id FROM connector.integration_grants WHERE fleet_id = $1 AND service = $2",
    )
    .bind(&live.ready.fleet)
    .bind(name)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
    .await
    .expect("the approved grant");
    afd_approval::IntegrationGrants::new(fixtures.database.clone(), Entropy::new())
        .revoke(&live.workspace, &live.ready.fleet, &grant)
        .await
        .expect("the person revokes");
}

// Value: protects=a card open for one turn holds none of the fleet's later
// turns; fails_when=a waiting request parks the fleet's queue; seam=the gate
// chain over live Postgres, a second message queued behind the asking turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_turn_after_a_waiting_request_runs() {
    let fixtures = Fixtures::create().await;
    let live = Live::issue(&fixtures).await;
    raised(&fixtures, &live).await;
    let later = crate::queue::enqueue(
        &fixtures.database,
        &live.ready.fleet,
        &live.workspace,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await;

    let next = live
        .next(&fixtures)
        .await
        .expect("the later turn is delivered");

    assert_eq!(
        next.pointer("/event/turn_id").and_then(Value::as_str),
        Some(later.as_str())
    );
    fixtures.cleanup().await;
}
