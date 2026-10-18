//! Settled setup chats share the tenant's cost history without private text.

use super::*;

#[path = "fixture.rs"]
mod fixture;
use fixture::Setup;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn setup_costs_page_with_runs_without_exposing_other_tenants_or_private_text() {
    let owner = Fixture::create().await;
    let other = Fixture::create().await;
    owner.seed().await;
    other.seed().await;
    let setup = Setup::seed(&owner).await;
    let foreign = Setup::seed(&other).await;
    let router = Fleet::live(
        owner.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();

    let rows = walk(&router, &owner).await;
    assert_scoped_history(&rows, &owner);
    assert_setup_row(&rows, &setup);
    let connection = owner.database.clone();
    let owned_runner = setup.runner;
    let foreign_runner = foreign.runner;
    assert_wallet_reads_its_balance(&router, &owner).await;
    owner.cleanup().await;
    other.cleanup().await;
    for runner in [owned_runner, foreign_runner] {
        sqlx::query("DELETE FROM fleet.runners WHERE id=$1::uuid")
            .bind(runner)
            .execute(&mut *connection.acquire().await.expect("cleanup connection"))
            .await
            .expect("scoped runner cleanup");
    }
}

fn assert_scoped_history(rows: &[Value], owner: &Fixture) {
    assert_eq!(
        rows.len(),
        4,
        "both run rows and both settled setup attempts"
    );
    assert_eq!(
        rows.iter()
            .map(|r| r["credit_deducted_nanos"].as_i64().expect("amount"))
            .sum::<i64>(),
        50
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r["charge_type"] == "authoring")
            .count(),
        2
    );
    for row in rows {
        assert_eq!(row["tenant_id"], owner.tenant, "foreign costs never leak");
        assert!(!row.as_object().expect("charge object").keys().any(|k| {
            ["title", "content", "answer", "model_snapshot", "usage"].contains(&k.as_str())
        }));
    }
}

async fn walk(router: &axum::Router, owner: &Fixture) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut next = Some(FIRST_CHARGE.to_owned());
    for _ in 0..6 {
        let Some(path) = next.take() else { break };
        let (status, page) = exchange(router, Method::GET, &path, Some(&owner.token), "").await;
        assert_eq!(status, StatusCode::OK);
        rows.extend(items(&page).iter().cloned());
        next = page
            .get("next_cursor")
            .and_then(Value::as_str)
            .map(|cursor| format!("{FIRST_CHARGE}&cursor={cursor}"));
    }
    assert!(next.is_none(), "the bounded history walk must terminate");
    rows
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn setup_costs_use_public_fleet_names_and_survive_private_chat_deletion() {
    let owner = Fixture::create().await;
    owner.seed().await;
    let setup = Setup::seed(&owner).await;
    let fleet = setup.link(&owner).await;
    let router = Fleet::live(
        owner.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();
    let before = walk(&router, &owner).await;
    let row = before
        .iter()
        .find(|r| r["id"] == setup.settlements[0])
        .expect("linked cost");
    assert_eq!(row["fleet_id"], fleet);
    assert_eq!(row["fleet_name"], "saved-setup");
    assert!(
        !serde_json::to_string(&before)
            .expect("wire values")
            .contains("private")
    );
    setup.remove_private_chat(&owner).await;
    let after = walk(&router, &owner).await;
    assert_eq!(after.len(), before.len());
    assert_setup_row(&after, &setup);
    let database = owner.database.clone();
    owner.cleanup().await;
    sqlx::query("DELETE FROM fleet.runners WHERE id=$1::uuid")
        .bind(setup.runner)
        .execute(&mut *database.acquire().await.expect("cleanup connection"))
        .await
        .expect("runner cleanup");
}

fn assert_setup_row(rows: &[Value], setup: &Setup) {
    let row = rows
        .iter()
        .find(|r| r["id"] == setup.settlements[0])
        .expect("settled setup row");
    assert_eq!(row["authoring_attempt_id"], setup.attempts[0]);
    assert_eq!(
        row["fleet_id"],
        Value::Null,
        "setup can precede fleet creation"
    );
    assert_eq!(row["event_id"], Value::Null, "setup is not a fleet event");
    assert_eq!(row["model"], "setup-model");
    assert_eq!(row["posture"], "platform");
    assert_eq!(row["token_count_input"], 23);
    assert_eq!(row["token_count_output"], 29);
    assert_eq!(row["wall_ms"], 31);
    assert_eq!(
        row["credit_deducted_nanos"], 17,
        "serve the actual debit, not a recalculated price"
    );
    let mut expected = setup.settlements.clone().to_vec();
    expected.extend(setup.ledger_rows.iter().cloned());
    expected.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_str().expect("id").to_owned())
            .collect::<Vec<_>>(),
        expected,
        "equal timestamps use UUID order across both sources, without duplicates or omissions"
    );
}
