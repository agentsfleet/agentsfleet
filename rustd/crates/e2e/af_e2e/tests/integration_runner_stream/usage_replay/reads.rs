//! Independent durable usage and wallet assertions for native renewal replay.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use super::fixture::Held;
use crate::{e2e::Scenario, e2e_seed::DEEP_POOL};
use af_core::{
    id::Uuid7,
    timing::{RENEWAL_TICK, STREAM_ANSWER_TIMEOUT, STREAM_RECOVERY_BUDGET},
};

const LINES: &str = "SELECT work_id, seq, input_token_count, output_token_count, charged_nanos, settled_at IS NOT NULL, kind FROM billing.usage_lines WHERE work_id = ANY($1) ORDER BY work_id, kind, seq";
type Line = (Uuid7, i64, i64, i64, i64, bool, String);

/// Whether `row` is one of the runner's model calls, by its stored kind.
fn is_model(row: &Line) -> bool {
    row.6 == af_wire::usage::LineKind::Model.as_str()
}

pub(super) async fn owner(scene: &Scenario) -> Uuid7 {
    sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id=$1")
        .bind(&scene.runner_id)
        .fetch_one(
            &mut *scene
                .booted
                .database
                .acquire()
                .await
                .expect("usage replay observer"),
        )
        .await
        .expect("authenticated runner owner")
}

pub(super) async fn changed_owner(scene: &Scenario, previous: &Uuid7) {
    tokio::time::timeout(STREAM_ANSWER_TIMEOUT + STREAM_RECOVERY_BUDGET, async {
        loop {
            if &owner(scene).await != previous {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("recovery confirms a new native connection");
}

async fn lines(scene: &Scenario, held: &[Held; 2]) -> Vec<Line> {
    let ids = [held[0].run.clone(), held[1].run.clone()];
    sqlx::query_as(LINES)
        .bind(&ids[..])
        .fetch_all(
            &mut *scene
                .booted
                .database
                .acquire()
                .await
                .expect("usage replay observer"),
        )
        .await
        .expect("durable usage lines")
}

fn counts(rows: &[Line], held: &[Held; 2]) -> [usize; 2] {
    held.each_ref().map(|run| {
        rows.iter()
            .filter(|row| row.0 == run.run && is_model(row))
            .count()
    })
}

pub(super) async fn wait_lines(scene: &Scenario, held: &[Held; 2], expected: [usize; 2]) {
    tokio::time::timeout(RENEWAL_TICK + STREAM_ANSWER_TIMEOUT, async {
        loop {
            if counts(&lines(scene, held).await, held) == expected {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both batched renewals commit before their answer is lost");
}

pub(super) async fn settled(scene: &Scenario, held: &[Held; 2], expected: [usize; 2]) {
    let rows = tokio::time::timeout(STREAM_ANSWER_TIMEOUT + STREAM_RECOVERY_BUDGET, async {
        loop {
            let rows = lines(scene, held).await;
            if counts(&rows, held) == expected && rows.iter().all(|row| row.5) {
                return rows;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("replayed renewals and final reports settle every usage line");
    provider_rows(&rows, held, expected);
    let total: i64 = rows.iter().map(|row| row.4).sum();
    let balance: i64 =
        sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1")
            .bind(&scene.tenant)
            .fetch_one(
                &mut *scene
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("usage replay observer"),
            )
            .await
            .expect("settled wallet balance");
    assert_eq!(
        balance,
        DEEP_POOL - total,
        "renewal replay and report replay charge each provider/runtime line once"
    );
}

fn provider_rows(rows: &[Line], held: &[Held; 2], expected: [usize; 2]) {
    for (index, run) in held.iter().enumerate() {
        let provider: Vec<_> = rows
            .iter()
            .filter(|row| row.0 == run.run && is_model(row))
            .collect();
        assert_eq!(
            provider.len(),
            *expected.get(index).expect("paired run count")
        );
        for (sequence, row) in provider.into_iter().enumerate() {
            assert_eq!(
                row.1,
                i64::try_from(sequence).expect("bounded fixture sequence"),
                "no omitted or duplicated provider sequence"
            );
            assert_eq!(
                (row.2, row.3, row.4),
                if index == 0 {
                    (100, 10, 450_000)
                } else {
                    (200, 10, 750_000)
                }
            );
        }
    }
}
