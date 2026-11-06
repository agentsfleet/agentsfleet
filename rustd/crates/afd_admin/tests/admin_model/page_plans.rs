//! Parameterized API-role plans over a realistically populated catalogue.
use super::{DbRole, Lane};
use afd_db::test_util::mint_id;
use serde_json::Value;
use sqlx::{Acquire as _, AssertSqlSafe};

const ROWS: usize = 4096;
const LIST: &str = afd_admin::test_util::MODEL_PAGE_QUERY;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn first_and_deep_pages_seek_the_identity_index_under_generic_plans() {
    super::isolated::run(async |lane| {
        let original = lane.file_model().await;
        seed_scale(lane, original.id.as_str()).await;
        let page = lane
            .models
            .page(50, None)
            .await
            .expect("priced page at scale");
        assert_eq!(page.models.len(), 50);
        assert!(
            page.models
                .iter()
                .all(|model| model.details.prices.len() == 1)
        );
        let migrator = lane.owner.open(DbRole::Migrator, &[]).await;
        let mut owner = migrator.acquire().await.expect("statistics owner");
        sqlx::query("ANALYZE model.platform_models")
            .execute(&mut *owner)
            .await
            .expect("fresh statistics");
        sqlx::query("ANALYZE model.decision_revisions")
            .execute(&mut *owner)
            .await
            .expect("revision statistics");
        drop(owner);
        migrator.close().await;
        for continuation in [false, true] {
            let plan = explain(lane, continuation).await;
            assert_plan(&plan);
        }
    })
    .await;
}

async fn seed_scale(lane: &Lane, original: &str) {
    let ids: Vec<String> = (0..ROWS).map(|_| mint_id()).collect();
    let mut connection = lane.database.acquire().await.expect("API seed connection");
    sqlx::query(
        "INSERT INTO model.platform_models
         (id, provider, model_name, display_name, context_cap_tokens, context_source_url,
          context_verified_at, advanced, created_at, updated_at)
         SELECT seeds.id::uuid, 'scale-provider', 'scale-' || lpad(seeds.ordinality::text, 5, '0'),
                m.display_name, m.context_cap_tokens, m.context_source_url,
                m.context_verified_at, m.advanced, now(), now()
         FROM unnest($1::text[]) WITH ORDINALITY AS seeds(id, ordinality)
         CROSS JOIN model.platform_models m WHERE m.id = $2::uuid",
    )
    .bind(&ids)
    .bind(original)
    .execute(&mut *connection)
    .await
    .expect("seed scale");
    seed_prices(&mut connection, &ids, original).await;
}

async fn explain(lane: &Lane, continuation: bool) -> Value {
    let mut connection = lane.database.acquire().await.expect("API role");
    let mut transaction = connection.begin().await.expect("begin plan");
    sqlx::query("SET LOCAL plan_cache_mode = force_generic_plan")
        .execute(&mut *transaction)
        .await
        .expect("generic prepared plan");
    sqlx::raw_sql(AssertSqlSafe(format!(
        "PREPARE admin_catalogue_page(bigint, text, text) AS {LIST}"
    )))
    .execute(&mut *transaction)
    .await
    .expect("prepare production query");
    let statement = if continuation {
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) EXECUTE admin_catalogue_page(51, 'scale-provider', 'scale-03000')"
    } else {
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) EXECUTE admin_catalogue_page(51, NULL, NULL)"
    };
    let plan: sqlx::types::Json<Value> = sqlx::query_scalar(statement)
        .fetch_one(&mut *transaction)
        .await
        .expect("actual API-role generic plan");
    sqlx::query("DEALLOCATE admin_catalogue_page")
        .execute(&mut *transaction)
        .await
        .expect("release prepared statement");
    transaction.rollback().await.expect("rollback plan");
    plan.0
}

fn identity_scan(plan: &Value) -> Option<&Value> {
    if plan["Index Name"] == "uq_platform_models_provider_model_name" {
        return Some(plan);
    }
    plan["Plans"].as_array()?.iter().find_map(identity_scan)
}

fn has_sort(plan: &Value) -> bool {
    plan["Node Type"]
        .as_str()
        .is_some_and(|kind| kind.contains("Sort"))
        || plan["Plans"]
            .as_array()
            .is_some_and(|children| children.iter().any(has_sort))
}

fn assert_plan(plan: &Value) {
    let root = &plan[0]["Plan"];
    assert_eq!(number(root, "Actual Rows"), 51.0, "{plan}");
    assert_eq!(number(root, "Actual Loops"), 1.0, "{plan}");
    let scan = identity_scan(root).expect("catalogue identity index serves the ordered page");
    assert_eq!(number(scan, "Actual Loops"), 1.0, "{plan}");
    let visited = (number(scan, "Actual Rows")
        + removed(scan, "Rows Removed by Filter", "Filter")
        + removed(scan, "Rows Removed by Index Recheck", "Recheck Cond"))
        * number(scan, "Actual Loops");
    assert!(
        visited <= 52.0,
        "deep pages must seek, not scan their prefix: {plan}"
    );
    assert!(
        !has_sort(root),
        "index order must avoid catalogue sorts: {plan}"
    );
    assert!(
        number(root, "Shared Hit Blocks") > 0.0,
        "BUFFERS evidence: {plan}"
    );
}

async fn seed_prices(connection: &mut sqlx::PgConnection, models: &[String], original: &str) {
    let ids: Vec<String> = models.iter().map(|_| mint_id()).collect();
    sqlx::query(
        "INSERT INTO model.platform_model_prices
         (id, platform_model_id, effective_from, effective_until, prompt_tokens_above,
          input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok,
          source_url, verified_at, conditions, created_at, updated_at)
         SELECT seeds.id::uuid, seeds.model::uuid, p.effective_from, p.effective_until,
                p.prompt_tokens_above, p.input_nanos_per_mtok, p.cached_input_nanos_per_mtok,
                p.output_nanos_per_mtok, p.source_url, p.verified_at, p.conditions, now(), now()
         FROM unnest($1::text[], $2::text[]) AS seeds(id, model)
         CROSS JOIN model.platform_model_prices p WHERE p.platform_model_id = $3::uuid",
    )
    .bind(ids)
    .bind(models)
    .bind(original)
    .execute(connection)
    .await
    .expect("seed scale prices");
}

fn number(node: &Value, field: &str) -> f64 {
    let value = node
        .get(field)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("missing or nonnumeric {field}: {node}"));
    assert!(value.is_finite() && value >= 0.0, "invalid {field}: {node}");
    value
}

fn removed(node: &Value, field: &str, predicate: &str) -> f64 {
    if node.get(field).is_some() {
        return number(node, field);
    }
    // PostgreSQL omits a removal counter when that predicate is absent.
    assert!(
        node.get(predicate).is_none(),
        "missing counter {field} for {predicate}: {node}"
    );
    0.0
}
