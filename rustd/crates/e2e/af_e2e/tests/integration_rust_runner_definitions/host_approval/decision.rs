//! Resolve the actual pending card and follow its continuation identifier.

use af_core::id::Uuid7;
use afd_approval::{Decision, Inbox, Resolution};
use serde_json::Value;

use super::script::{DEST, ORIGIN, Outcome};
use crate::e2e::Scenario;

pub(super) async fn answer(scene: &Scenario, outcome: Outcome) -> Uuid7 {
    let db = scene.booted.database.clone();
    let mut connection = db.acquire().await.expect("gate connection");
    let (action, evidence): (Uuid7, sqlx::types::Json<Value>) = sqlx::query_as(
        "SELECT action_id, evidence FROM fleet.approval_gates WHERE fleet_id=$1 AND turn_id=$2 AND status='pending'",
    ).bind(&scene.fleet).bind(&scene.turn_id).fetch_one(&mut *connection).await.expect("asking turn raised a pending card");
    assert_eq!(
        evidence.0["redirect"],
        serde_json::json!({"tool":"web_fetch","from_host":ORIGIN,"to_host":DEST})
    );
    assert!(!evidence.0.to_string().contains("fixture-private"));
    drop(connection);
    let bus = scene.booted.bus.clone();
    let inbox = Inbox::new(
        db.clone(),
        bus.clone(),
        afd_turns::Turns::for_tests(db.clone(), bus),
    );
    let decision = if outcome == Outcome::Denied {
        Decision::Denied
    } else {
        Decision::Approved
    };
    let resolved = inbox
        .resolve(
            action.as_str(),
            decision,
            "fixture:reviewer",
            "",
            Some(scene.fleet.as_str()),
        )
        .await
        .expect("inbox decision");
    let Resolution::Resolved(resolved) = resolved else {
        unreachable!("an unanswered gate resolves: {resolved:?}")
    };
    assert_eq!(resolved.turn_id, Some(scene.turn_id.clone()));
    let expected = if outcome == Outcome::Denied {
        "revoked"
    } else {
        "approved"
    };
    let status: String = sqlx::query_scalar("SELECT status FROM connector.integration_grants WHERE fleet_id=$1 AND service=$2 AND grant_type='host'")
        .bind(&scene.fleet).bind(DEST).fetch_one(&mut *db.acquire().await.expect("grant connection")).await.expect("host grant");
    assert_eq!(status, expected);
    resolved
        .continuation_turn_id
        .expect("the granted or revoked host continues the waiting turn")
}
