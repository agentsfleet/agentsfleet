//! Each queued message and host belongs to this proof; the shared queue is untouched.

use super::{Fixture, mint, now};
use afd_authoring::{
    Claimed,
    types::{Draft, MessageQuery},
};
use afd_core::id::Uuid7;
use afd_db::test_util::FleetSeed;

/// The status the runtime fleet carries.
const FLEET_ACTIVE: &str = "active";

pub(super) async fn funded(workers: i32) -> Fixture {
    fund(Fixture::create().await, workers).await
}

pub(super) async fn fund(f: Fixture, workers: i32) -> Fixture {
    sqlx::query("INSERT INTO fleet.runners(id,host_id,token_hash,sandbox_tier,admin_state,labels,network_policy,registry_allowlist,worker_count,degraded,last_seen_at,created_at,updated_at) VALUES($1::uuid,$1,$1,'dev_none','active','[]','deny_all_egress','[]',$2,false,$3,$3,$3)")
        .bind(f.runner.as_str()).bind(workers).bind(now().as_millis())
        .execute(&mut *f.connection().await).await.expect("owned host");
    sqlx::query("INSERT INTO billing.tenant_wallet(tenant_id,balance_nanos,grant_source,created_at,updated_at) VALUES($1::uuid,$3,'authoring-fixture',$2,$2)")
        .bind(f.tenant.as_str()).bind(now().as_millis()).bind(crate::draft::FUNDED_NANOS)
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
    sqlx::query_scalar("SELECT count(*) FROM fleet.authoring_attempts WHERE workspace_id=$1::uuid")
        .bind(f.workspace.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned obligations")
}

pub(super) async fn balance(f: &Fixture, nanos: i64) {
    sqlx::query("UPDATE billing.tenant_wallet SET balance_nanos=$2 WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .bind(nanos)
        .execute(&mut *f.connection().await)
        .await
        .expect("controlled owned wallet");
}

pub(super) async fn runtime(f: &Fixture, until: i64) -> Uuid7 {
    let fleet = mint();
    FleetSeed::new(
        fleet.as_str(),
        f.workspace.as_str(),
        f.tenant.as_str(),
        FLEET_ACTIVE,
    )
    .name("capacity-runtime")
    .at(now().as_millis())
    .insert(&mut *f.connection().await)
    .await
    .expect("owned runtime fleet");
    sqlx::query("INSERT INTO fleet.runner_leases(id,runner_id,fleet_id,workspace_id,tenant_id,event_id,receipt,actor,event_type,event_created_at,posture,provider,model,metered_input_tokens,metered_cached_tokens,metered_output_tokens,last_metered_at,fencing_token,lease_expires_at,status,created_at,updated_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$1,'1-0','fixture','chat',$6,'tenant','test','test',0,0,0,$6,1,$7,'active',$6,$6)")
        .bind(mint().as_str()).bind(f.runner.as_str()).bind(fleet.as_str())
        .bind(f.workspace.as_str()).bind(f.tenant.as_str()).bind(now().as_millis()).bind(until)
        .execute(&mut *f.connection().await).await.expect("owned active runtime slot");
    fleet
}
