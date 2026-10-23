//! Dimension 4.1 — the draft and fleet deletion roots keep the money and scrub
//! private output at once; an open draft and an installing fleet survive their
//! sweeps.
//!
//! Both roots in one test, because the property is one matrix read from two
//! sides: deleting a draft and purging a fleet each leave the attempt, its
//! settlement and every `billing.usage_lines` row behind, now naming a parent
//! that no longer exists. Lives beside the setup fixture, which is the one place
//! a test reaches a real installing fleet and the purge's live queue together.

use super::super::{attempt::Attempt, fixture::now, mint};
use super::fixture::Case;
use afd_authoring::types::{Source, Start};
use afd_billing::Posture;
use afd_billing::sql::work::{AUTHORING_ATTEMPT, FLEET_LEASE};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::Fleets;
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};

/// The private output a dispatch holds until an erasure root scrubs it.
const PRIVATE_RESULT: &str = r#"{"text":"private model output"}"#;

/// What the fleet's settled charge cost.
const FLEET_CHARGE_NANOS: i64 = 1_700;

/// The status a setup fleet holds until its activation is funded.
const INSTALLING: &str = "installing";

/// A terminal report that settles one runtime millisecond.
fn settling_report() -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: "1".into(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
        answer: Some("A retained private answer.".into()),
    }
}

/// `(attempts with an outcome, usage lines, settled usage lines)` for `attempt`.
async fn attempt_money(c: &Case, attempt: &Uuid7) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM authoring.attempts WHERE id=$1::uuid AND outcome IS NOT NULL), \
         (SELECT count(*) FROM billing.usage_lines WHERE work_type=$2 AND work_id=$1::uuid), \
         (SELECT count(settled_at) FROM billing.usage_lines WHERE work_type=$2 AND work_id=$1::uuid)",
    )
    .bind(attempt.as_str())
    .bind(AUTHORING_ATTEMPT)
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("the attempt's money reads")
}

/// `(dispatches, with a result, with a native reply)` for `attempt`.
async fn dispatch_output(c: &Case, attempt: &Uuid7) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT count(*), count(result), count(native_reply) \
         FROM authoring.dispatches WHERE attempt_id=$1::uuid",
    )
    .bind(attempt.as_str())
    .fetch_one(&mut *c.base.connection().await)
    .await
    .expect("the attempt's dispatches read")
}

/// How many rows of `table` carry `id`. The table name is a literal from this file.
async fn rows(c: &Case, table: &str, id: &Uuid7) -> i64 {
    let statement = sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table} WHERE id=$1::uuid"));
    sqlx::query_scalar(statement)
        .bind(id.as_str())
        .fetch_one(&mut *c.base.connection().await)
        .await
        .expect("the row count reads")
}

/// One read of a single text column of one row, by id.
async fn column(c: &Case, statement: &'static str, id: &Uuid7) -> Option<String> {
    sqlx::query_scalar(statement)
        .bind(id.as_str())
        .fetch_optional(&mut *c.base.connection().await)
        .await
        .expect("the column reads")
}

/// A model call's dispatch, holding private output, on `attempt`.
async fn dispatch_with_output(c: &Case, attempt: &Uuid7) {
    sqlx::query(
        "INSERT INTO authoring.dispatches (id, attempt_id, sequence, request_digest, status, \
         result, usage, native_reply, private_expires_at, dispatch_expires_at, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, 1, 'fixture', 'known', $3::jsonb, '{}'::jsonb, $3::jsonb, \
         now() + interval '1 day', now() + interval '30 seconds', now(), now())",
    )
    .bind(mint().as_str())
    .bind(attempt.as_str())
    .bind(PRIVATE_RESULT)
    .execute(&mut *c.base.connection().await)
    .await
    .expect("the dispatch is recorded");
}

/// One settled usage line charged to `fleet`, answering its id.
async fn fleet_charge(c: &Case, fleet: &Uuid7) -> Uuid7 {
    let line = mint();
    sqlx::query(
        "INSERT INTO billing.usage_lines
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name,
            event_id, authoring_attempt_id, provider, model, posture, requested_at,
            input_token_count, cached_input_token_count, cache_write_token_count,
            cache_write_5m_token_count, cache_write_1h_token_count, output_token_count,
            runtime_ms, platform_model_price_id, unpriced_token_count, charged_nanos,
            settled_at, created_at, updated_at)
         VALUES ($1::uuid, $5, $1::uuid, 0, $2::uuid, $3::uuid, $4::uuid,
                 (SELECT f.name FROM fleet.fleets f WHERE f.id = $4::uuid),
                 $1, NULL, 'anthropic', 'claude-opus-5', $6, now(), 0, 0, 0, 0, 0, 0, 0,
                 NULL, 0, $7, now(), now(), now())",
    )
    .bind(line.as_str())
    .bind(c.base.tenant.as_str())
    .bind(c.base.workspace.as_str())
    .bind(fleet.as_str())
    .bind(FLEET_LEASE)
    .bind(Posture::Platform.as_str())
    .bind(FLEET_CHARGE_NANOS)
    .execute(&mut *c.base.connection().await)
    .await
    .expect("the ledger accepts a fleet charge");
    line
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_deletion_matrix_keeps_money() {
    let c = Case::create().await;
    let fleets = Fleets::new(c.base.db.clone());
    let (attempt, money) = the_draft_root_keeps_money(&c).await;
    the_fleet_root_keeps_money(&c, &fleets).await;
    assert_eq!(
        attempt_money(&c, &attempt).await,
        money,
        "the purge took no draft money"
    );
    the_sweeps_leave_open_work(&c, &fleets).await;
    c.cleanup().await;
}

/// Deletes a draft holding a settled attempt whose dispatch still holds
/// output; answers the attempt and the money it must keep.
async fn the_draft_root_keeps_money(c: &Case) -> (Uuid7, (i64, i64, i64)) {
    let a = Attempt::start(&c.base, c.base.access(), Source::Blank).await;
    let reported = c
        .base
        .drafts
        .report(&c.base.runner, &a.lease, settling_report())
        .await
        .expect("the report settles");
    assert!(reported.settled, "known usage settles");
    dispatch_with_output(c, &a.id).await;
    let money = attempt_money(c, &a.id).await;
    assert_eq!(money.0, 1, "the attempt carries its outcome");
    assert!(money.1 > 0, "the settlement logged usage lines: {money:?}");
    assert_eq!(money.1, money.2, "every line is settled: {money:?}");
    assert_eq!(dispatch_output(c, &a.id).await, (1, 1, 1));

    c.base
        .drafts
        .delete(c.base.access(), &a.draft.id)
        .await
        .expect("the creator deletes the draft");
    assert_eq!(rows(c, "authoring.drafts", &a.draft.id).await, 0);
    assert_eq!(
        dispatch_output(c, &a.id).await,
        (1, 0, 0),
        "the delete scrubs dispatches.result at once and keeps the row"
    );
    assert_eq!(attempt_money(c, &a.id).await, money, "the money stays");
    assert_eq!(
        column(
            c,
            "SELECT draft_id::text FROM authoring.attempts WHERE id=$1::uuid",
            &a.id
        )
        .await,
        Some(a.draft.id.to_string()),
        "the attempt keeps the deleted draft's id, now opaque"
    );
    (a.id, money)
}

/// Purges a killed setup fleet with a settled charge.
async fn the_fleet_root_keeps_money(c: &Case, fleets: &Fleets) {
    let (_, request) = c.request("matrix-purged", Start::None).await;
    let saved = c.save(&mint(), request).await;
    assert_eq!(saved.status, INSTALLING);
    let line = fleet_charge(c, &saved.id).await;
    let killed = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await
        .expect("the setup fleet is killed");
    fleets
        .purge_tagged(&c.base.workspace, &saved.id, &killed.etag)
        .await
        .expect("a killed fleet purges");
    assert_eq!(rows(c, "fleet.fleets", &saved.id).await, 0);
    assert_eq!(
        column(
            c,
            "SELECT fleet_id::text FROM billing.usage_lines WHERE id=$1::uuid",
            &line
        )
        .await,
        Some(saved.id.to_string()),
        "the charge outlives its fleet and still names it"
    );
    assert_eq!(
        column(
            c,
            "SELECT (settled_at IS NOT NULL)::text FROM billing.usage_lines WHERE id=$1::uuid",
            &line
        )
        .await
        .as_deref(),
        Some("true"),
        "its settlement stays"
    );
}

/// What the sweeps must leave: an open draft, and a closed draft whose fleet
/// is still installing, and the installing fleet itself.
async fn the_sweeps_leave_open_work(c: &Case, fleets: &Fleets) {
    let open = c
        .base
        .drafts
        .create(c.base.access(), c.base.request(mint(), None), now())
        .await
        .expect("an open draft")
        .draft;
    let (setup_draft, request) = c.request("matrix-installing", Start::None).await;
    let installing = c.save(&mint(), request).await;
    assert_eq!(installing.status, INSTALLING);
    assert_eq!(
        column(
            c,
            "SELECT fleet_id::text FROM authoring.drafts WHERE id=$1::uuid",
            &setup_draft
        )
        .await,
        Some(installing.id.to_string()),
        "the setup draft names its installing fleet"
    );
    for (draft, status) in [(&open.id, None), (&setup_draft, Some("closed"))] {
        sqlx::query(
            "UPDATE authoring.drafts SET status=COALESCE($2, status), expires_at='epoch' \
             WHERE id=$1::uuid",
        )
        .bind(draft.as_str())
        .bind(status)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("the draft is past retention");
    }
    c.base
        .drafts
        .maintain()
        .await
        .expect("the draft sweep runs");
    assert_eq!(
        rows(c, "authoring.drafts", &open.id).await,
        1,
        "an open draft stays"
    );
    assert_eq!(
        rows(c, "authoring.drafts", &setup_draft).await,
        1,
        "a draft whose fleet is installing stays"
    );
    let refused = fleets
        .purge_tagged(&c.base.workspace, &installing.id, &installing.etag)
        .await
        .expect_err("an installing fleet is not purged");
    assert_eq!(refused.code(), error_code::AGENTSFLEET_ALREADY_TERMINAL);
    assert_eq!(
        column(
            c,
            "SELECT status FROM fleet.fleets WHERE id=$1::uuid",
            &installing.id
        )
        .await,
        Some(INSTALLING.to_owned()),
        "the installing fleet is untouched"
    );
}
