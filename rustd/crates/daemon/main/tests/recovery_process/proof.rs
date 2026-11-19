//! Assertions observe the committed checkpoint, original expiry and replacement settlement.

use super::{ACCEPTED, COMPLETED, Checked};
use crate::e2e::Scenario;
use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;
use serde_json::Value;
use std::time::Duration;

pub(super) struct Winner {
    run: Uuid7,
    fence: i64,
}

pub(super) fn prefix(body: &Value, expected: &[&str]) {
    let text = body
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("content"))
        .flat_map(|content| match content {
            Value::String(text) => vec![text.as_str()],
            Value::Array(parts) => parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    for word in expected {
        assert_eq!(
            text.matches(word).count(),
            1,
            "the restored model input contains each accepted item once: {word}"
        );
    }
}

pub(super) async fn checkpoint(scene: &Scenario, covered: &Uuid7) -> Checked<Uuid7> {
    let mut connection = scene.booted.database.acquire().await?;
    let (id, ready, sealed, folds): (Uuid7, bool, bool, Vec<Uuid7>) = sqlx::query_as(
        "SELECT p.id,p.ready_at IS NOT NULL,p.root_secret_ciphertext IS NOT NULL,p.covered_fold_ids
        FROM runner.turn_recovery h JOIN runner.turn_checkpoints p ON p.id=h.head_turn_checkpoint_id WHERE h.event_id=$1")
        .bind(&scene.event_id).fetch_one(&mut *connection).await?;
    assert!(ready && sealed);
    assert_eq!(folds, vec![covered.clone()]);
    Ok(id)
}

pub(super) async fn deadline(scene: &Scenario) -> Checked<jiff::Timestamp> {
    let mut connection = scene.booted.database.acquire().await?;
    let expiry: jiff_sqlx::Timestamp = sqlx::query_scalar(
        "SELECT GREATEST(l.run_expires_at,s.run_expires_at) FROM thread.runs l
        JOIN thread.threads s ON s.fleet_id=l.fleet_id
        WHERE l.event_id=$1 AND l.runner_id=$2 ORDER BY l.created_at DESC LIMIT 1",
    )
    .bind(&scene.event_id)
    .bind(&scene.runner_id)
    .fetch_one(&mut *connection)
    .await?;
    Ok(expiry.to_jiff())
}

pub(super) async fn no_early_issue(
    scene: &Scenario,
    runner: &Uuid7,
    deadline: jiff::Timestamp,
) -> Checked {
    tokio::time::timeout(Duration::from_secs(75), async {
        loop {
            let mut connection = scene.booted.database.acquire().await?;
            let (live, issued): (bool, bool) = sqlx::query_as(
                "SELECT clock_timestamp()<$3::timestamptz,EXISTS(
                    SELECT 1 FROM thread.runs WHERE event_id=$1 AND runner_id=$2)",
            )
            .bind(&scene.event_id)
            .bind(runner)
            .bind(deadline.to_sqlx())
            .fetch_one(&mut *connection)
            .await?;
            if !live {
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
            }
            assert!(
                !issued,
                "a dead host's current run still owns the original deadline"
            );
            drop(connection);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    Ok(())
}

pub(super) async fn replacement(
    scene: &Scenario,
    runner: &Uuid7,
    deadline: jiff::Timestamp,
) -> Checked<Winner> {
    let mut connection = scene.booted.database.acquire().await?;
    let (run, fence, previous, issued): (Uuid7, i64, i64, jiff_sqlx::Timestamp) = sqlx::query_as(
        "SELECT b.id,b.fencing_token,a.fencing_token,b.created_at FROM thread.runs b
        JOIN thread.runs a ON a.event_id=b.event_id AND a.runner_id=$3
        WHERE b.event_id=$1 AND b.runner_id=$2",
    )
    .bind(&scene.event_id)
    .bind(runner)
    .bind(&scene.runner_id)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(fence, previous + 1);
    assert!(issued.to_jiff() >= deadline);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE event_id=$1")
        .bind(&scene.event_id)
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(count, 2);
    Ok(Winner { run, fence })
}

pub(super) async fn adopted(scene: &Scenario, winner: &Winner, ids: &[Uuid7]) -> Checked {
    let mut connection = scene.booted.database.acquire().await?;
    let claims: Vec<(Uuid7, i64, bool)> = sqlx::query_as(
        "SELECT input_claim_run_id,input_claim_fence,settled_at IS NOT NULL FROM fleet.admissions WHERE id=ANY($1::uuid[])")
        .bind(ids).fetch_all(&mut *connection).await?;
    assert_eq!(claims.len(), 2);
    assert!(
        claims
            .iter()
            .all(|(run, fence, settled)| run == &winner.run && *fence == winner.fence && !settled)
    );
    Ok(())
}

pub(super) async fn settled(scene: &Scenario, winner: &Winner, folds: &[Uuid7]) -> Checked {
    let outcome = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let mut connection = scene.booted.database.acquire().await?;
            let result: Option<String> = sqlx::query_scalar(
                "SELECT response_text FROM fleet.events WHERE event_id=$1 AND status=$2",
            )
            .bind(&scene.event_id)
            .bind(af_core::event::status::PROCESSED)
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(answer) = result {
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(answer);
            }
            drop(connection);
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    assert_eq!(outcome, format!("{ACCEPTED}\n\n{COMPLETED}"));
    let mut connection = scene.booted.database.acquire().await?;
    let responses: Vec<String> = sqlx::query_scalar(
        "SELECT response_text FROM fleet.events WHERE event_id=ANY($1::uuid[]) AND status=$2",
    )
    .bind(folds)
    .bind(af_core::event::status::PROCESSED)
    .fetch_all(&mut *connection)
    .await?;
    assert_eq!(responses, vec![outcome.clone(), outcome]);
    let usage: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT input_token_count,output_token_count FROM billing.usage_lines WHERE work_id=$1 AND kind=$2")
        .bind(&winner.run).bind(af_wire::usage::LineKind::Model.as_str()).fetch_all(&mut *connection).await?;
    assert_eq!(
        usage,
        vec![(321, 123)],
        "restored answers never rebill the old attempt's model tokens"
    );
    Ok(())
}

pub(super) async fn quarantined(scene: &Scenario) -> Checked {
    tokio::time::timeout(Duration::from_secs(145), async {
        loop {
            let mut connection = scene.booted.database.acquire().await?;
            let (status, reason): (String, Option<String>) =
                sqlx::query_as("SELECT status,cordon_reason FROM runner.runners WHERE id=$1")
                    .bind(&scene.runner_id)
                    .fetch_one(&mut *connection)
                    .await?;
            if let Some(reason) = reason {
                assert_eq!(status, afd_state::sql::RUNNER_STATUS_ACTIVE);
                assert_eq!(reason, "missed_heartbeats");
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
            }
            drop(connection);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await??;
    let mut connection = scene.booted.database.acquire().await?;
    let events: Vec<(String, i64)> = sqlx::query_as(
        "SELECT event_type,count(*) FROM runner.history WHERE runner_id=$1 AND event_type=ANY($2::text[]) GROUP BY event_type ORDER BY event_type")
        .bind(&scene.runner_id).bind(["runner_cordoned", "runner_offline"])
        .fetch_all(&mut *connection).await?;
    assert_eq!(
        events,
        vec![
            ("runner_cordoned".to_owned(), 1),
            ("runner_offline".to_owned(), 1)
        ]
    );
    Ok(())
}
