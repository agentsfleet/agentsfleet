//! Queue polling must not scan a fleet's unrelated approval history.

use afd_db::config::DbRole;
use sqlx::Connection as _;

use super::{Fixtures, prepared, relation_scans, run};

/// A dense existence check may choose a scan, but must stay within a few pages
/// as unrelated history doubles. This also catches the former bitmap build.
const DENSE_GATE_BUFFER_BUDGET: f64 = 16.0;

// Mix resolved gates, expired matching gates and live gates for other events.
const HISTORY: &str = "INSERT INTO fleet.approval_gates \
    (id,fleet_id,workspace_id,event_id,action_id,tool_name,action_name,gate_type, \
     proposed_action,evidence,blast_radius,expires_at,status,detail,resolved_at, \
     created_at,updated_at) \
    SELECT uuidv7(),a.fleet_id,a.workspace_id, \
      CASE WHEN n%3=0 THEN 'unrelated-'||n ELSE a.id::text END, \
      'history-'||n,'git','push','repository_write','fixture','{}','fixture', \
      clock_timestamp()+CASE WHEN n%3=1 THEN interval '-1 hour' ELSE interval '1 hour' END, \
      $4,'',CASE WHEN n%3=2 THEN now() END,now(),now() \
    FROM fleet.admissions a,generate_series($2::int,$3::int) n WHERE a.id=$1::uuid";

// Value: protects=approval work stays bounded as history grows; fails_when=a gate
// probe filters unrelated or expired rows; why_new=parked-head deadline lookup;
// seam=real generic Postgres plans; no_claim=constant total cost across all fleets.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parked_head_probes_skip_unrelated_and_expired_approval_history() {
    let fixtures = Fixtures::create_isolated().await;
    let seeded = crate::seed::seeded::<1>(&fixtures).await;
    let [runner] = &seeded.runners;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("isolated plan transaction");
    prepared::prepare(&mut tx).await;
    sqlx::query("UPDATE fleet.admissions SET parked_at=now() WHERE id=$1::uuid")
        .bind(&seeded.event_id)
        .execute(&mut *tx)
        .await
        .expect("park head");
    let mut previous = 0;
    for size in [2000, 4000, 8000, 16000] {
        sqlx::query(HISTORY)
            .bind(&seeded.event_id)
            .bind(previous + 1)
            .bind(size)
            .bind(afd_api_wire::approval::status::PENDING)
            .execute(&mut *tx)
            .await
            .expect("grow approval history");
        run(&mut tx, "ANALYZE fleet.approval_gates").await;
        probe_plans(&mut tx, &seeded.fleet, runner.as_str(), size).await;
        previous = size;
    }
    assert_dense_matches(&mut tx, &seeded.event_id, &seeded.fleet, runner.as_str()).await;
    assert_other_fleet_history(&mut tx, &seeded, runner.as_str()).await;
    prepared::release(&mut tx).await;
    tx.rollback().await.expect("discard history");
    drop(connection);
    owner.close().await;
    fixtures.cleanup().await;
}

async fn probe_plans(connection: &mut sqlx::PgConnection, fleet: &str, runner: &str, size: i32) {
    for mode in ["force_custom_plan", "force_generic_plan"] {
        run(connection, &format!("SET LOCAL plan_cache_mode={mode}")).await;
        let discovery = prepared::candidates(connection, runner).await;
        run(connection, "SAVEPOINT claim_probe").await;
        let claim = prepared::claim(connection, fleet, runner).await;
        run(connection, "ROLLBACK TO SAVEPOINT claim_probe").await;
        for (name, plan) in [("discovery", discovery), ("claim", claim)] {
            assert_gate_work(&plan);
            if mode == "force_generic_plan" {
                assert!(
                    plan.to_string().contains("$1"),
                    "expected parameterized plan: {plan}"
                );
            }
            eprintln!(
                "{name}: mode={mode}, gates={size}, ms={}, shared_hits={}, shared_reads={}",
                plan[0]["Execution Time"],
                plan[0]["Plan"]["Shared Hit Blocks"],
                plan[0]["Plan"]["Shared Read Blocks"]
            );
        }
    }
}

async fn assert_dense_matches(
    connection: &mut sqlx::PgConnection,
    event: &str,
    fleet: &str,
    runner: &str,
) {
    sqlx::query("UPDATE fleet.approval_gates SET event_id=$1, expires_at=clock_timestamp()+interval '1 hour' WHERE fleet_id=$2::uuid AND resolved_at IS NULL")
        .bind(event).bind(fleet).execute(&mut *connection).await.expect("many live matching gates");
    run(connection, "ANALYZE fleet.approval_gates").await;
    let live = prepared::candidates(connection, runner).await;
    assert_dense_work(&live);
}

fn assert_dense_work(plan: &serde_json::Value) {
    let scans = relation_scans(plan, "approval_gates");
    assert!(
        scans
            .iter()
            .any(|scan| scan["Actual Rows"].as_f64() == Some(1.0)),
        "{plan}"
    );
    for scan in scans {
        // A dense match can favor a sequential scan; either plan must stop
        // after the first live gate instead of collecting matching rows.
        assert!(
            scan["Actual Rows"].as_f64().unwrap_or_default() <= 1.0,
            "{scan}"
        );
        assert_bounded_rows(scan);
        let buffers = scan["Shared Hit Blocks"].as_f64().unwrap_or_default()
            + scan["Shared Read Blocks"].as_f64().unwrap_or_default();
        assert!(buffers <= DENSE_GATE_BUFFER_BUDGET, "{scan}");
    }
}

fn assert_gate_work(plan: &serde_json::Value) {
    let scans = relation_scans(plan, "approval_gates");
    assert!(!scans.is_empty(), "parked head checks approvals: {plan}");
    assert!(
        scans
            .iter()
            .any(|scan| scan["Actual Loops"].as_f64().unwrap_or_default() > 0.0),
        "the deadline probe must run: {plan}"
    );
    for scan in scans {
        eprintln!(
            "gate probe: node={}, index={}, bounds={}, rows={}, filtered={}, loops={}",
            scan["Node Type"],
            scan["Index Name"],
            scan["Index Cond"],
            scan["Actual Rows"],
            scan["Rows Removed by Filter"],
            scan["Actual Loops"]
        );
        assert!(
            scan["Rows Removed by Filter"].as_f64().unwrap_or_default() <= 1.0,
            "approval history is filtered after reading it: {plan}"
        );
        assert!(
            scan["Actual Rows"].as_f64().unwrap_or_default() <= 1.0,
            "approval existence probes stop at the first match: {plan}"
        );
        assert_ne!(scan["Node Type"], "Seq Scan", "indexed gate scopes: {plan}");
        assert_index_work(scan);
    }
}

async fn assert_other_fleet_history(
    connection: &mut sqlx::PgConnection,
    seeded: &crate::seed::Seeded<1>,
    runner: &str,
) {
    let other = afd_db::test_util::mint_id();
    let workspace: String =
        sqlx::query_scalar("SELECT workspace_id::text FROM fleet.fleets WHERE id=$1::uuid")
            .bind(&seeded.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("workspace");
    afd_db::test_util::FleetSeed::new(
        &other,
        &workspace,
        &seeded.tenant,
        afd_fleet::lease::sql::FLEET_STATUS_ACTIVE,
    )
    .name(&other)
    .insert(&mut *connection)
    .await
    .expect("unrelated fleet");
    for copies in [1_i32, 1, 2] {
        sqlx::query("INSERT INTO fleet.approval_gates SELECT (jsonb_populate_record(NULL::fleet.approval_gates,to_jsonb(g)||jsonb_build_object('id',uuidv7(),'fleet_id',$2::uuid,'event_id','unrelated','expires_at',clock_timestamp()+interval '30 minutes'))).* FROM fleet.approval_gates g,generate_series(1,$3) WHERE g.fleet_id=$1::uuid AND g.resolved_at IS NULL")
            .bind(&seeded.fleet).bind(&other).bind(copies).execute(&mut *connection).await.expect("unrelated earlier deadlines");
        run(connection, "ANALYZE fleet.approval_gates").await;
        let plan = prepared::candidates(connection, runner).await;
        assert_dense_work(&plan);
        eprintln!(
            "dense cross-fleet probe: ms={}, shared_hits={}",
            plan[0]["Execution Time"], plan[0]["Plan"]["Shared Hit Blocks"]
        );
    }
}

fn assert_index_work(node: &serde_json::Value) {
    if node.get("Index Name").is_some() {
        assert_eq!(
            node["Index Name"], "idx_approval_gates_pending_scope_deadline",
            "{node}"
        );
        let bounds = node["Index Cond"].as_str().expect("index bounds");
        assert!(
            ["fleet_id", "event_id"]
                .iter()
                .all(|key| bounds.contains(key)),
            "{node}"
        );
        assert!(
            bounds.contains("IS NULL") || bounds.contains("expires_at"),
            "{node}"
        );
        assert!(
            node["Actual Rows"].as_f64().unwrap_or_default() <= 1.0,
            "existence must stop at one row: {node}"
        );
        eprintln!(
            "gate index: bounds={bounds}, rows={}, loops={}",
            node["Actual Rows"], node["Actual Loops"]
        );
    }
    if let Some(children) = node["Plans"].as_array() {
        for child in children {
            assert_index_work(child);
        }
    }
}

fn assert_bounded_rows(node: &serde_json::Value) {
    assert!(
        node["Actual Rows"].as_f64().unwrap_or_default() <= 1.0,
        "{node}"
    );
    if let Some(children) = node["Plans"].as_array() {
        for child in children {
            assert_bounded_rows(child);
        }
    }
}
