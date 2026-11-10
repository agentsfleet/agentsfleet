//! Each queued message and host belongs to this proof; the shared queue is untouched.

use super::{Fixture, mint, now};
use af_core::id::Uuid7;
use afd_authoring::{
    Claimed,
    types::{Draft, MessageQuery},
};
use afd_db::test_util::{EventKind, FleetEventSeed, FleetSeed, RunnerPolicy, RunnerSeed};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

/// The status the runtime fleet and the host carry.
const ACTIVE: &str = "active";

/// The host's isolation: no sandbox and no egress.
const POLICY: RunnerPolicy<'static> = RunnerPolicy {
    sandbox_tier: "dev_none",
    network_policy: "deny_all_egress",
};

pub(super) async fn funded(workers: i32) -> Fixture {
    fund(Fixture::create().await, workers).await
}

pub(super) async fn fund(f: Fixture, workers: i32) -> Fixture {
    RunnerSeed::new(&f.runner, ACTIVE, POLICY)
        .workers(workers)
        .insert(&mut *f.connection().await)
        .await
        .expect("owned host");
    sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1,$2,now(),now())")
        .bind(&f.tenant).bind(crate::draft::FUNDED_NANOS)
        .execute(&mut *f.connection().await).await.expect("owned wallet");
    f
}

pub(super) async fn queue(f: &Fixture, text: &str) -> Draft {
    f.drafts
        .create(f.access(), f.request(mint(), Some(text)), now())
        .await
        .expect("queue native candidate")
        .draft
}

pub(super) async fn claim(f: &Fixture, draft: &Draft) -> Option<Claimed> {
    f.drafts
        .claim_queued(&f.runner, &draft.id)
        .await
        .expect("admission answers")
}

pub(super) async fn failure(f: &Fixture, draft: &Draft) -> afd_authoring::types::Message {
    f.drafts
        .messages(
            f.access(),
            &draft.id,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("reload durable failure")
        .items
        .into_iter()
        .find(|m| m.role == "assistant")
        .expect("terminal assistant projection")
}

pub(super) async fn attempts(f: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM authoring.attempts WHERE workspace_id=$1")
        .bind(&f.workspace)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned obligations")
}

pub(super) async fn balance(f: &Fixture, nanos: i64) {
    sqlx::query("UPDATE billing.wallets SET balance_nanos=$2 WHERE tenant_id=$1")
        .bind(&f.tenant)
        .bind(nanos)
        .execute(&mut *f.connection().await)
        .await
        .expect("controlled owned wallet");
}

pub(super) async fn runtime(f: &Fixture, until: Timestamp) -> Uuid7 {
    let fleet = mint();
    FleetSeed::new(&fleet, &f.workspace, &f.tenant, ACTIVE)
        .name("capacity-runtime")
        .at(now())
        .insert(&mut *f.connection().await)
        .await
        .expect("owned runtime fleet");
    let kind = EventKind {
        actor: "fixture",
        event_type: "chat",
        status: ACTIVE,
    };
    let event = mint();
    FleetEventSeed::new(&fleet, &f.workspace, &event, kind)
        .insert(&mut *f.connection().await)
        .await
        .expect("owned runtime event");
    sqlx::query("INSERT INTO runner.leases(id,runner_id,fleet_id,workspace_id,tenant_id,event_id,event_created_at,posture,provider,model,last_metered_at,fencing_token,lease_expires_at,status,posted_message_count,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,now(),'tenant','test','test',now(),1,$7,'active',0,now(),now())")
        .bind(mint()).bind(&f.runner).bind(&fleet)
        .bind(&f.workspace).bind(&f.tenant).bind(&event).bind(until.to_sqlx())
        .execute(&mut *f.connection().await).await.expect("owned active runtime slot");
    fleet
}
