//! Dimension 3.4: setup work never reaches another tenant's runner.

use super::{
    Fixture,
    fixture::{funded, queue},
    mint,
};
use af_core::id::Uuid7;
use afd_setup::Drafts;
use sqlx::Row as _;

/// A host owned by `tenant`, able to take one attempt.
async fn tenant_runner(f: &Fixture, tenant: &Uuid7) -> Uuid7 {
    let runner = mint();
    sqlx::query(
        "INSERT INTO runner.runners (id, host_id, token_hash, sandbox_tier, status, labels, \
         network_policy, registry_allowlist, worker_count, last_seen_at, run_version, \
         created_at, updated_at, platform_owned, tenant_id) \
         VALUES ($1, 'tenant-host', $1::text, 'dev_none', 'active', '[]'::jsonb, \
                 'deny_all_egress', '[]'::jsonb, 2, now(), 0, now(), now(), false, $2)",
    )
    .bind(&runner)
    .bind(tenant)
    .execute(&mut *f.connection().await)
    .await
    .expect("the tenant's host inserts");
    runner
}

/// The drafts the candidate read assigns `runner`.
async fn assigned(f: &Fixture, runner: &Uuid7) -> Vec<Uuid7> {
    sqlx::query(Drafts::QUEUED_DRAFTS_QUERY)
        .bind("editing")
        .bind("queued")
        .bind(["awaiting_accounting", "settled"])
        .bind(64_i64)
        .bind(runner)
        .bind("cancelled")
        .fetch_all(&mut *f.connection().await)
        .await
        .expect("the candidate read answers")
        .iter()
        .map(|row| row.get("id"))
        .collect()
}

async fn scalar(f: &Fixture, statement: &'static str, id: &Uuid7) -> i64 {
    sqlx::query_scalar(statement)
        .bind(id)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("the count reads")
}

// Value: protects=a draft's setup chat runs only on its tenant's or a platform
// host; fails_when=the candidate read or `admit` drops the owner; why_new=§3's
// authoring filter; seam=none.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn authoring_placement_refuses_a_foreign_runner() {
    let f = funded(2).await;
    let draft = queue(&f, "Draft a support triage fleet").await;
    let foreign = tenant_runner(&f, &f.member_tenant).await;
    let own = tenant_runner(&f, &f.tenant).await;

    assert!(
        !assigned(&f, &foreign).await.contains(&draft.id),
        "never assigned to a foreign host"
    );
    assert!(
        assigned(&f, &own).await.contains(&draft.id),
        "assigned to its tenant's host"
    );

    let captured = af_core::test_util::trace::Capture::install();
    let refused = f
        .drafts
        .assign_queued(&foreign, &draft.id)
        .await
        .expect("assignment answers");
    let logged = captured.only("runner_placement_refused");
    drop(captured);
    assert!(refused.is_none(), "a foreign host is assigned nothing");
    assert_eq!(logged.field("run_type"), Some("authoring"));
    assert_eq!(logged.field("reason"), Some("foreign_tenant"));
    assert_eq!(
        scalar(
            &f,
            "SELECT count(*) FROM setup.attempts WHERE runner_id = $1",
            &foreign
        )
        .await,
        0,
        "no attempt names the foreign host"
    );
    assert_eq!(
        scalar(
            &f,
            "SELECT count(*) FROM thread.turns WHERE thread_id = $1 AND status = 'queued'",
            &draft.id
        )
        .await,
        1,
        "the message stays queued"
    );

    let placed = f
        .drafts
        .assign_queued(&own, &draft.id)
        .await
        .expect("assignment answers");
    assert!(
        placed.is_some(),
        "the tenant's own host is then assigned it"
    );

    sqlx::query("DELETE FROM runner.runners WHERE id = ANY($1)")
        .bind([&foreign, &own])
        .execute(&mut *f.connection().await)
        .await
        .expect("the hosts clean up");
    f.cleanup().await;
}
