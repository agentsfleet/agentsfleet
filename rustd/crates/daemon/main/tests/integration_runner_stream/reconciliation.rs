//! Opening reconciles durable leases without replaying an omitted lease before it lapses.

use super::{isolation, population, raw::Native};
use crate::{e2e::Scenario, wire};
use af_core::{error_code, id::Uuid7};
use af_stream::Transport;
use af_wire::{
    lease::TaggedLease,
    protocol::{Down, StreamKind},
};
use afd_db::{Db, test_util::TestDatabase};
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};

type Proof<T = ()> = af_stream::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Lease {
    id: Uuid7,
    event: Uuid7,
    fence: u64,
}

struct Case {
    supervisor: Supervisor,
    database: TestDatabase,
    owner: Db,
    run: Scenario,
    siblings: [(Uuid7, Uuid7); 2],
    held: [Lease; 3],
}

impl Case {
    async fn open() -> Proof<Self> {
        let mut supervisor = Supervisor::new();
        let (database, owner, mut run) = isolation::open(&mut supervisor, &[]).await;
        let siblings = [
            population::sibling(&run, "Pull request review").await,
            population::sibling(&run, "Release review").await,
        ];
        sqlx::query("UPDATE runner.runners SET worker_count=$2 WHERE id=$1")
            .bind(&run.runner_id)
            .bind(3_i32)
            .execute(&mut *run.booted.database.acquire().await?)
            .await?;
        let held = acquire(&run).await?;
        run.native
            .get_mut()
            .ok_or("the original session was not opened")?
            .close()
            .await?;
        Ok(Self {
            supervisor,
            database,
            owner,
            run,
            siblings,
            held,
        })
    }

    fn lease(&self, event: &Uuid7) -> Proof<&Lease> {
        self.held
            .iter()
            .find(|lease| &lease.event == event)
            .ok_or_else(|| "the expected event was not offered".into())
    }

    async fn close(self) {
        self.supervisor.shutdown().await;
        for (fleet, _) in &self.siblings {
            crate::e2e_retire::retire_fleet(&self.run.booted, fleet).await;
        }
        self.run.cleanup().await;
        self.owner.close().await;
        self.database.cleanup().await;
    }
}

async fn acquire(run: &Scenario) -> Proof<[Lease; 3]> {
    let session = run.native_session().await;
    session.capacity(3);
    wire::heartbeat(run, &wire::capable_beat()).await;
    Ok([
        offered(session).await?,
        offered(session).await?,
        offered(session).await?,
    ])
}

async fn offered(session: &Session) -> Proof<Lease> {
    let frame = tokio::time::timeout(Duration::from_secs(5), session.offer()).await??;
    let message: Down<'_> = frame.decode()?;
    let Down::Offer {
        lease: TaggedLease::Fleet(lease),
    } = message
    else {
        return Err("the native offer was not fleet work".into());
    };
    Ok(Lease {
        id: Uuid7::parse(&lease.lease_id)?,
        event: Uuid7::parse(&lease.event.event_id)?,
        fence: lease.fencing_token,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn opening_reconciles_retained_missing_and_cancelled_work_without_reoffering_omitted_work()
-> Proof {
    for transport in [Transport::H2, Transport::H3] {
        reconcile(transport).await?;
    }
    Ok(())
}

async fn reconcile(transport: Transport) -> Proof {
    let case = Case::open().await?;
    let listed = case.lease(&case.run.event_id)?;
    let [(omitted_fleet, omitted_event), (_, marked_event)] = &case.siblings;
    let omitted = case.lease(omitted_event)?;
    let marked = case.lease(marked_event)?;
    sqlx::query("UPDATE runner.leases SET cancel_requested_at=statement_timestamp() WHERE id=$1")
        .bind(&marked.id)
        .execute(&mut *case.run.booted.database.acquire().await?)
        .await?;
    let missing = afd_db::test_util::mint_id();
    let mut current = Native::connect_running(
        case.run.booted.runner_stream_address,
        &case.run.token,
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
        return Err("an omitted live lease cannot be offered before its expiry".into());
    }
    expire(&case.run, omitted_fleet, &omitted.id).await?;
    let bus = case.run.booted.bus.clone();
    afd_bus::wake::publish(&bus, omitted_fleet.clone()).await;
    let offered = current.expect(StreamKind::Work, "offer").await;
    assert_eq!(
        wire::pointer(&offered, "/lease/event/event_id"),
        omitted_event.as_str()
    );
    let (replacement, fence) = wire::claim(wire::pointer(&offered, "/lease"));
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
                .get("lease_id")
                .and_then(serde_json::Value::as_str)
                .zip(frame.get("error_code").and_then(serde_json::Value::as_str))
        })
        .collect();
    assert_eq!(
        cancelled,
        BTreeSet::from([
            Some((missing.as_str(), error_code::RUN_LEASE_LOST.as_str())),
            Some((marked.as_str(), error_code::RUN_LEASE_CANCELLED.as_str()))
        ])
    );
}

async fn assert_retained_renews(native: &mut Native, lease: &Uuid7) {
    native
        .send(
            StreamKind::Control,
            &json!({"type":"renew_batch", "request_id":7,
        "leases":[{"lease_id":lease,"request":{"requests":[]}}]}),
        )
        .await;
    let reply = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(wire::pointer(&reply, "/leases/0/lease_id"), lease.as_str());
    assert_eq!(wire::pointer(&reply, "/leases/0/answer/status"), "ok");
}

async fn expire(run: &Scenario, fleet: &Uuid7, lease: &Uuid7) -> Proof {
    let mut connection = run.booted.database.acquire().await?;
    sqlx::query("UPDATE runner.leases SET lease_expires_at=statement_timestamp()-interval '1 second' WHERE id=$1")
        .bind(lease).execute(&mut *connection).await?;
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=statement_timestamp()-interval '1 second' WHERE fleet_id=$1")
        .bind(fleet).execute(&mut *connection).await?;
    Ok(())
}
