//! Private fixture ownership, statement fingerprints and a complete sequenced activity census.

use super::super::raw::Native;
use super::Result;
use crate::{e2e::Scenario, wire};
use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_wire::protocol::StreamKind;
use afd_bus::{Handle, Received, Subject, Subscription};
use afd_db::{Db, test_util::TestDatabase};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) struct Fixture {
    database: TestDatabase,
    pub(super) owner: Db,
    pub(super) scene: Scenario,
}

impl Fixture {
    pub(super) async fn open(supervisor: &mut Supervisor) -> Self {
        let (database, owner, scene) = super::super::isolation::open(supervisor, &[]).await;
        Self {
            database,
            owner,
            scene,
        }
    }

    pub(super) async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.scene.cleanup().await;
        self.owner.close().await;
        self.database.cleanup().await;
    }
}

pub(super) struct Held {
    pub(super) id: String,
    pub(super) fence: u64,
}

pub(super) async fn healthy(native: &mut Native, request_id: u64) {
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"heartbeat", "request_id":request_id, "request":wire::capable_beat()
            }),
        )
        .await;
    let response = native.expect(StreamKind::Control, "heartbeat").await;
    assert_eq!(response.get("request_id"), Some(&json!(request_id)));
    assert_eq!(response.pointer("/answer/status"), Some(&json!("ok")));
    assert_eq!(
        response.pointer("/answer/response/degraded"),
        Some(&json!(false))
    );
}

pub(super) async fn offer(scene: &Scenario, native: &mut Native) -> Result<Held> {
    let response = native.expect(StreamKind::Work, "run").await;
    let run = response
        .get("run")
        .ok_or("the native offer lacks its run")?;
    assert_eq!(run.pointer("/event/turn_id"), Some(&json!(scene.turn_id)));
    let (id, fence) = wire::claim(run);
    Ok(Held { id, fence })
}

// afd_state/src/sql/select_runner_token.sql; other owner checks read by runner id.
const TOKEN: &str = "%SELECT id AS runner, status, degraded_reason IS NOT NULL AS degraded%FROM runner.runners%WHERE token_hash =%";
// afd_fleet/src/run/sql/select_run_target.sql; a native cache hit avoids this join.
const TARGET: &str = "%SELECT l.fleet_id, l.turn_id, l.created_at, l.turn_created_at,%FROM thread.runs l%JOIN thread.turns e ON e.id = l.turn_id%WHERE l.id =%";
// afd_runner/src/capacity/sql/lock_host.sql; Ready must reach this durable guard.
const HOST: &str = "%SELECT worker_count, status, cordon_reason, degraded_reason IS NOT NULL AS degraded, run_version,%FROM runner.runners%WHERE id =%FOR UPDATE%";
// afd_fleet/src/run/sql/select_run_for_report.sql; a session that kept
// the offer's facts reports without it.
const REPORTED: &str = "%SELECT l.fleet_id, l.workspace_id,%l.tenant_id, l.turn_id, e.actor, l.posture%FROM thread.runs l%WHERE l.id = $1 AND l.runner_id = $2%";
const COUNTS: &str = "SELECT \
    COALESCE(sum(calls) FILTER (WHERE query LIKE $1), 0)::bigint, \
    COALESCE(sum(calls) FILTER (WHERE query LIKE $2), 0)::bigint, \
    COALESCE(sum(calls) FILTER (WHERE query LIKE $3), 0)::bigint \
    FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())";

pub(super) struct Counts {
    pub(super) tokens: i64,
    pub(super) targets: i64,
    pub(super) report_reads: i64,
}

impl Counts {
    pub(super) async fn read(owner: &Db) -> Result<Self> {
        let (tokens, targets, report_reads) = sqlx::query_as(COUNTS)
            .bind(TOKEN)
            .bind(TARGET)
            .bind(REPORTED)
            .fetch_one(&mut *owner.acquire().await?)
            .await?;
        Ok(Self {
            tokens,
            targets,
            report_reads,
        })
    }
}

pub(super) async fn host_locks(owner: &Db) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(sum(calls), 0)::bigint FROM pg_stat_statements \
        WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
          AND query LIKE $1",
    )
    .bind(HOST)
    .fetch_one(&mut *owner.acquire().await?)
    .await?)
}

pub(super) struct Tail {
    bus: Handle,
    subscription: Subscription,
    event: String,
    seen: HashSet<u64>,
}

impl Tail {
    pub(super) async fn open(scene: &Scenario) -> Result<Self> {
        let bus = afd_bus::test_util::lane().await?;
        let subscription = bus
            .subscribe(&Subject::FleetTail(scene.fleet.clone()))
            .await?;
        crate::tail::settle().await;
        Ok(Self {
            bus,
            subscription,
            event: scene.turn_id.to_string(),
            seen: HashSet::new(),
        })
    }

    pub(super) async fn observe(&mut self, sequence: u64) -> Result<()> {
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT, self.receive_chunk(sequence)).await??;
        Ok(())
    }

    async fn receive_chunk(&mut self, sequence: u64) -> Result<()> {
        loop {
            let Received::Message(message) = self.subscription.recv().await? else {
                return Err("a gap or lag cannot establish the activity census".into());
            };
            let frame: Value = serde_json::from_str(&message.payload)?;
            if frame.get("kind").and_then(Value::as_str) != Some("chunk") {
                continue;
            }
            assert_eq!(
                frame.get("turn_id").and_then(Value::as_str),
                Some(self.event.as_str())
            );
            assert_eq!(
                frame.get("stream_seq").and_then(Value::as_u64),
                Some(sequence)
            );
            assert_eq!(
                frame.get("text").and_then(Value::as_str),
                Some(text(sequence).as_str())
            );
            assert!(
                self.seen.insert(sequence),
                "each activity frame publishes once"
            );
            return Ok(());
        }
    }

    pub(super) fn count(&self) -> usize {
        self.seen.len()
    }

    pub(super) async fn close(self) -> Result<()> {
        drop(self.subscription);
        self.bus.shutdown().await?;
        Ok(())
    }
}

pub(super) fn text(sequence: u64) -> String {
    format!("native-cost-{sequence}")
}
