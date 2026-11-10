//! Private fixture ownership, statement fingerprints and a complete sequenced activity census.

use super::super::raw::Native;
use super::Result;
use crate::{
    e2e::{Scenario, dragonfly_config},
    wire,
};
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
    pub(super) run: Scenario,
}

impl Fixture {
    pub(super) async fn open(supervisor: &mut Supervisor) -> Self {
        let (database, owner, run) = super::super::isolation::open(supervisor, &[]).await;
        Self {
            database,
            owner,
            run,
        }
    }

    pub(super) async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.run.cleanup().await;
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

pub(super) async fn offer(run: &Scenario, native: &mut Native) -> Result<Held> {
    let response = native.expect(StreamKind::Work, "offer").await;
    let lease = response
        .get("lease")
        .ok_or("the native offer lacks its lease")?;
    assert_eq!(lease.pointer("/event/event_id"), Some(&json!(run.event_id)));
    let (id, fence) = wire::claim(lease);
    Ok(Held { id, fence })
}

// afd_state/src/sql/select_runner_token.sql; other owner checks read by runner id.
const TOKEN: &str = "%SELECT id AS runner, status, degraded_reason IS NOT NULL AS degraded%FROM runner.runners%WHERE token_hash =%";
// afd_fleet/src/lease/sql/select_lease_target.sql; a native cache hit avoids this join.
const TARGET: &str = "%SELECT l.fleet_id, l.event_id, l.created_at, l.event_created_at,%FROM runner.leases l%JOIN fleet.events e ON e.event_id = l.event_id%WHERE l.id =%";
// afd_runner/src/capacity/sql/lock_host.sql; Ready must reach this durable guard.
const HOST: &str = "%SELECT worker_count, status, degraded_reason IS NOT NULL AS degraded, lease_version,%FROM runner.runners%WHERE id =%FOR UPDATE%";
const COUNTS: &str = "SELECT \
    COALESCE(sum(calls) FILTER (WHERE query LIKE $1), 0)::bigint, \
    COALESCE(sum(calls) FILTER (WHERE query LIKE $2), 0)::bigint \
    FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())";

pub(super) struct Counts {
    pub(super) tokens: i64,
    pub(super) targets: i64,
}

impl Counts {
    pub(super) async fn read(owner: &Db) -> Result<Self> {
        let (tokens, targets) = sqlx::query_as(COUNTS)
            .bind(TOKEN)
            .bind(TARGET)
            .fetch_one(&mut *owner.acquire().await?)
            .await?;
        Ok(Self { tokens, targets })
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
    pub(super) async fn open(run: &Scenario) -> Result<Self> {
        let bus = afd_bus::dragonfly::connect(run.booted.queue.clone(), dragonfly_config()).await?;
        let subscription = bus
            .subscribe(&Subject::FleetTail(run.fleet.clone()))
            .await?;
        crate::tail::settle().await;
        Ok(Self {
            bus,
            subscription,
            event: run.event_id.to_string(),
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
                frame.get("event_id").and_then(Value::as_str),
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
