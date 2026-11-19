//! Opening reconciles durable runs without replaying an omitted run before it lapses.

use super::{isolation, population, raw::Native};
use crate::{e2e::Scenario, wire};
use af_core::{error_code, id::Uuid7};
use af_stream::Transport;
use af_wire::{
    protocol::{Down, StreamKind},
    run::TaggedRun,
};
use afd_db::{Db, test_util::TestDatabase};
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};

type Proof<T = ()> = af_stream::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Run {
    id: Uuid7,
    event: Uuid7,
    fence: u64,
}

struct Case {
    supervisor: Supervisor,
    database: TestDatabase,
    owner: Db,
    scene: Scenario,
    siblings: [(Uuid7, Uuid7); 2],
    held: [Run; 3],
}

impl Case {
    async fn open() -> Proof<Self> {
        let mut supervisor = Supervisor::new();
        let (database, owner, mut scene) = isolation::open(&mut supervisor, &[]).await;
        let siblings = [
            population::sibling(&scene, "Pull request review").await,
            population::sibling(&scene, "Release review").await,
        ];
        sqlx::query("UPDATE runner.runners SET worker_count=$2 WHERE id=$1")
            .bind(&scene.runner_id)
            .bind(3_i32)
            .execute(&mut *scene.booted.database.acquire().await?)
            .await?;
        let held = acquire(&scene).await?;
        scene
            .native
            .get_mut()
            .ok_or("the original session was not opened")?
            .close()
            .await?;
        Ok(Self {
            supervisor,
            database,
            owner,
            scene,
            siblings,
            held,
        })
    }

    fn run(&self, event: &Uuid7) -> Proof<&Run> {
        self.held
            .iter()
            .find(|run| &run.event == event)
            .ok_or_else(|| "the expected event was not offered".into())
    }

    async fn close(self) {
        self.supervisor.shutdown().await;
        for (fleet, _) in &self.siblings {
            crate::e2e_retire::retire_fleet(&self.scene.booted, fleet).await;
        }
        self.scene.cleanup().await;
        self.owner.close().await;
        self.database.cleanup().await;
    }
}

async fn acquire(scene: &Scenario) -> Proof<[Run; 3]> {
    let session = scene.native_session().await;
    session.capacity(3);
    wire::heartbeat(scene, &wire::capable_beat()).await;
    Ok([
        offered(session).await?,
        offered(session).await?,
        offered(session).await?,
    ])
}

async fn offered(session: &Session) -> Proof<Run> {
    let frame = tokio::time::timeout(Duration::from_secs(5), session.offer()).await??;
    let message: Down<'_> = frame.decode()?;
    let Down::Run {
        run: TaggedRun::Fleet(run),
    } = message
    else {
        return Err("the native offer was not fleet work".into());
    };
    Ok(Run {
        id: Uuid7::parse(&run.run_id)?,
        event: Uuid7::parse(&run.event.event_id)?,
        fence: run.fencing_token,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn opening_reconciles_retained_missing_and_cancelled_work_without_reoffering_omitted_work()
-> Proof {
    for transport in [Transport::H2, Transport::H3] {
        Box::pin(reconcile(transport)).await?;
    }
    Ok(())
}

async fn reconcile(transport: Transport) -> Proof {
    let case = Case::open().await?;
    let listed = case.run(&case.scene.event_id)?;
    let [(omitted_fleet, omitted_event), (_, marked_event)] = &case.siblings;
    let omitted = case.run(omitted_event)?;
    let marked = case.run(marked_event)?;
    sqlx::query("UPDATE runner.runs SET cancel_requested_at=statement_timestamp() WHERE id=$1")
        .bind(&marked.id)
        .execute(&mut *case.scene.booted.database.acquire().await?)
        .await?;
    let missing = afd_db::test_util::mint_id();
    let mut current = Native::connect_running(
        case.scene.booted.runner_stream_address,
        &case.scene.token,
        transport,
        &[listed.id.clone(), missing.clone(), marked.id.clone()],
    )
    .await;
    assert_cancelled(&current, &missing, &marked.id);
    assert_retained_renews(&mut current, &listed.id).await;
    current.ready().await;
    if tokio::time::timeout(Duration::from_millis(250), current.next(StreamKind::Work))
        .await
        .is_ok()
    {
        return Err("an omitted live run cannot be offered before its expiry".into());
    }
    expire(&case.scene, omitted_fleet, &omitted.id).await?;
    let bus = case.scene.booted.bus.clone();
    afd_bus::wake::publish(&bus, omitted_fleet.clone()).await;
    let offered = current.expect(StreamKind::Work, "offer").await;
    assert_eq!(
        wire::pointer(&offered, "/run/event/event_id"),
        omitted_event.as_str()
    );
    let (replacement, fence) = wire::claim(wire::pointer(&offered, "/run"));
    assert_ne!(replacement, omitted.id.as_str());
    assert!(fence > omitted.fence);
    current.close().await;
    case.close().await;
    Ok(())
}

fn assert_cancelled(native: &Native, missing: &Uuid7, marked: &Uuid7) {
    let cancelled: BTreeSet<_> = native
        .cancelled
        .iter()
        .map(|frame| {
            frame
                .get("run_id")
                .and_then(serde_json::Value::as_str)
                .zip(frame.get("error_code").and_then(serde_json::Value::as_str))
        })
        .collect();
    assert_eq!(
        cancelled,
        BTreeSet::from([
            Some((missing.as_str(), error_code::RUN_LOST.as_str())),
            Some((marked.as_str(), error_code::RUN_CANCELLED.as_str()))
        ])
    );
}

async fn assert_retained_renews(native: &mut Native, run: &Uuid7) {
    native
        .send(
            StreamKind::Control,
            &json!({"type":"renew_batch", "request_id":7,
        "runs":[{"run_id":run,"request":{"requests":[]}}]}),
        )
        .await;
    let reply = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(wire::pointer(&reply, "/runs/0/run_id"), run.as_str());
    assert_eq!(wire::pointer(&reply, "/runs/0/answer/status"), "ok");
}

async fn expire(scene: &Scenario, fleet: &Uuid7, run: &Uuid7) -> Proof {
    let mut connection = scene.booted.database.acquire().await?;
    sqlx::query("UPDATE runner.runs SET run_expires_at=statement_timestamp()-interval '1 second' WHERE id=$1")
        .bind(run).execute(&mut *connection).await?;
    sqlx::query("UPDATE runner.slots SET run_expires_at=statement_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(fleet).execute(&mut *connection).await?;
    Ok(())
}
