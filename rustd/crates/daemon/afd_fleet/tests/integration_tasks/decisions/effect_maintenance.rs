//! Expiring read custody reconciles an issued mutation without a live task owner.
use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
    github_authority,
    github_server::Server,
    publication_refusal::prepared,
    publication_transport::preparation,
};
use af_wire::task::TaskEffectState;
use afd_fleet::task::TaskLifecycle as _;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
enum Outcome {
    Confirmed,
    Unknown,
    Failed,
    Expired,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn lost_publication_acknowledgement_is_reconciled_read_only_or_retains_uncertainty() {
    for outcome in [
        Outcome::Confirmed,
        Outcome::Unknown,
        Outcome::Failed,
        Outcome::Expired,
    ] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let server = Server::start(vec![]).await;
            let tasks = github_authority::publisher(&fixture, &server).await;
            let input = garde::Unvalidated::new(prepared(case, task).await)
                .validate()
                .expect("bounded exact-head publication");
            preparation(&server);
            server.push(
                json!({"fixture_request":{"method":"POST","path":"/graphql","publication":true},
                "response":{"errors":[{"message":"publication acknowledgement unavailable"}]}}),
            );
            server.push(history(&json!([])));
            let (receipt, replayed) = tasks
                .publish_effect(case.lease(), &input)
                .await
                .expect("issued effect remains discoverable");
            assert!(!replayed);
            assert_eq!(receipt.state, TaskEffectState::Unknown);
            assert_eq!(receipt.ordinal, 1);
            assert!(receipt.commit.is_none());
            assert_eq!(
                stored(case, &receipt.effect_id).await,
                ("unknown".to_owned(), None, 1, 0, 1)
            );
            reconcile(case, &tasks, &server, &receipt, outcome).await;
            if matches!(outcome, Outcome::Confirmed) {
                let (again, replayed) = tasks
                    .publish_effect(case.lease(), &input)
                    .await
                    .expect("terminal receipt replay");
                assert!(replayed);
                assert_eq!(again.effect_id, receipt.effect_id);
                assert_eq!(again.commit, Some("3".repeat(40)));
            }
            server.finish().await;
            fixture.model.finish().await.expect("paid decision cleanup");
        })
        .await;
    }
}

async fn reconcile(
    case: &Case,
    tasks: &afd_fleet::task::Tasks,
    server: &Server,
    receipt: &af_wire::task::TaskEffect,
    outcome: Outcome,
) {
    due(
        case,
        &receipt.effect_id,
        matches!(outcome, Outcome::Expired),
    )
    .await;
    observation(server, &receipt.effect_id, outcome);
    let result = tasks
        .maintain_effects()
        .await
        .expect("bounded read custody sweep");
    assert_eq!(result.scanned, 1);
    assert_eq!(
        (
            result.confirmed,
            result.unknown,
            result.failed,
            result.expired
        ),
        match outcome {
            Outcome::Confirmed => (1, 0, 0, 0),
            Outcome::Unknown => (0, 1, 0, 0),
            Outcome::Failed => (0, 0, 1, 0),
            Outcome::Expired => (0, 0, 0, 1),
        }
    );
    let expected = match outcome {
        Outcome::Confirmed => ("confirmed".to_owned(), Some("3".repeat(40)), 1, 1, 0),
        Outcome::Expired => ("unknown".to_owned(), None, 1, 0, 0),
        Outcome::Unknown | Outcome::Failed => ("unknown".to_owned(), None, 1, 0, 1),
    };
    assert_eq!(stored(case, &receipt.effect_id).await, expected);
    assert_eq!(
        tasks
            .maintain_effects()
            .await
            .expect("no immediate redispatch")
            .scanned,
        0
    );
    assert_eq!(stored(case, &receipt.effect_id).await, expected);
}

fn observation(server: &Server, effect: &str, outcome: Outcome) {
    match outcome {
        Outcome::Confirmed => server.push(history(&json!([{"oid":"3".repeat(40),
            "message":format!("Repair\n\nagentsfleet-effect:{effect}"),
            "tree":{"oid":"2".repeat(40)},"parents":{"nodes":[{"oid":"0".repeat(40)}]}}]))),
        Outcome::Unknown => server.push(history(&json!([]))),
        Outcome::Failed => server.push(json!({"errors":[{"message":"history unavailable"}]})),
        Outcome::Expired => {}
    }
}

fn history(nodes: &Value) -> Value {
    json!({"data":{"repository":{"databaseId":101,"ref":{"target":{"history":{"nodes":nodes}}}}}})
}

async fn due(case: &Case, effect: &str, expired: bool) {
    let changed = sqlx::query("UPDATE fleet.task_effect_recovery SET due_at=clock_timestamp()-interval '1 second',credential_expires_at=CASE WHEN $2 THEN clock_timestamp()-interval '1 second' ELSE credential_expires_at END WHERE task_effect_id=$1::uuid")
        .bind(effect).bind(expired).execute(&mut *case.held.fixtures.database.acquire().await.expect("reconciliation clock fixture"))
        .await.expect("advance only the owned custody clock");
    assert_eq!(changed.rows_affected(), 1);
}

async fn stored(case: &Case, effect: &str) -> (String, Option<String>, i32, i64, i64) {
    sqlx::query_as("SELECT s.status,s.commit_oid,t.used_publications,t.confirmed_effect_watermark,(SELECT count(*) FROM fleet.task_effect_recovery r WHERE r.task_effect_id=e.id) FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.task_effect_id=e.id JOIN fleet.tasks t ON t.id=e.task_id WHERE e.id=$1::uuid")
        .bind(effect).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("effect observer"))
        .await.expect("publication attempt and exact receipt persisted together")
}
