//! Each queued message and host belongs to this proof; the shared queue is untouched.

use super::{Fixture, mint, now};
use afd_authoring::{
    Claimed,
    types::{Draft, MessageQuery},
};
use afd_core::id::Uuid7;
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
    RunnerSeed::new(f.runner.as_str(), ACTIVE, POLICY)
        .workers(workers)
        .insert(&mut *f.connection().await)
        .await
        .expect("owned host");
    sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
        .bind(f.tenant.as_str()).bind(crate::draft::FUNDED_NANOS)
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
    sqlx::query_scalar("SELECT count(*) FROM authoring.attempts WHERE workspace_id=$1::uuid")
        .bind(f.workspace.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned obligations")
}

pub(super) async fn balance(f: &Fixture, nanos: i64) {
    sqlx::query("UPDATE billing.wallets SET balance_nanos=$2 WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .bind(nanos)
        .execute(&mut *f.connection().await)
        .await
        .expect("controlled owned wallet");
}

pub(super) async fn runtime(f: &Fixture, until: Timestamp) -> Uuid7 {
    let fleet = mint();
    FleetSeed::new(
        fleet.as_str(),
        f.workspace.as_str(),
        f.tenant.as_str(),
        ACTIVE,
    )
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
    let event = FleetEventSeed::new(fleet.as_str(), f.workspace.as_str(), "1-0", kind)
        .insert(&mut *f.connection().await)
        .await
        .expect("owned runtime event");
    sqlx::query("INSERT INTO runner.leases(id,runner_id,fleet_id,workspace_id,tenant_id,fleet_event_id,event_created_at,receipt,posture,provider,model,last_metered_at,fencing_token,lease_expires_at,status,posted_message_count,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$6::uuid,now(),'1-0','tenant','test','test',now(),1,$7,'active',0,now(),now())")
        .bind(mint().as_str()).bind(f.runner.as_str()).bind(fleet.as_str())
        .bind(f.workspace.as_str()).bind(f.tenant.as_str()).bind(&event).bind(until.to_sqlx())
        .execute(&mut *f.connection().await).await.expect("owned active runtime slot");
    fleet
}
