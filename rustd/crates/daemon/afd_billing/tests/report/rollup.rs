//! The daily rollup as a test reads it: its rows, the same sums computed from
//! the settled ledger, and everything one settlement writes.

use af_core::id::Uuid7;
use af_wire::usage::LineKind;
use afd_db::Db;

use super::fixture::{balance, ledger};

/// The rollup rows of one tenant, in key order.
const SELECT_USAGE_DAILY: &str = "\
SELECT day::text AS day, work_type, fleet_id, model, charged_nanos, input_tokens,
       output_tokens, runtime_ms, line_count, unpriced_tokens
  FROM billing.usage_daily WHERE tenant_id = $1
 ORDER BY day, work_type, fleet_id NULLS FIRST, model";

/// The same key and sums, computed from the settled ledger lines alone.
const SELECT_LEDGER_DAILY: &str = "\
SELECT (created_at AT TIME ZONE 'UTC')::date::text AS day, work_type, fleet_id, model,
       SUM(charged_nanos)::bigint AS charged_nanos, SUM(input_token_count)::bigint AS input_tokens,
       SUM(output_token_count)::bigint AS output_tokens, SUM(runtime_ms)::bigint AS runtime_ms,
       COUNT(*)::bigint AS line_count, SUM(unpriced_token_count)::bigint AS unpriced_tokens
  FROM billing.usage_lines WHERE tenant_id = $1 AND settled_at IS NOT NULL
 GROUP BY 1, work_type, fleet_id, model
 ORDER BY 1, work_type, fleet_id NULLS FIRST, model";

/// The run counts of one tenant, in key order.
const SELECT_RUNS_DAILY: &str = "\
SELECT day::text AS day, work_type, fleet_id, runs FROM billing.runs_daily WHERE tenant_id = $1
 ORDER BY day, work_type, fleet_id NULLS FIRST";

/// A run counted from the ledger: its smallest runtime line, once that line is
/// settled, on that line's UTC day.
const SELECT_LEDGER_RUNS: &str = "\
SELECT (first.created_at AT TIME ZONE 'UTC')::date::text AS day, first.work_type,
       first.fleet_id, COUNT(*)::bigint AS runs
  FROM (SELECT DISTINCT ON (work_type, work_id) work_type, fleet_id, created_at, settled_at
          FROM billing.usage_lines WHERE tenant_id = $1 AND kind = $2
         ORDER BY work_type, work_id, seq) AS first
 WHERE first.settled_at IS NOT NULL
 GROUP BY 1, first.work_type, first.fleet_id
 ORDER BY 1, first.work_type, first.fleet_id NULLS FIRST";

/// One `billing.usage_daily` key and its sums.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(super) struct DailyRow {
    pub(super) day: String,
    pub(super) work_type: String,
    pub(super) fleet_id: Option<Uuid7>,
    pub(super) model: String,
    pub(super) charged_nanos: i64,
    pub(super) input_tokens: i64,
    pub(super) output_tokens: i64,
    pub(super) runtime_ms: i64,
    pub(super) line_count: i64,
    pub(super) unpriced_tokens: i64,
}

/// One `billing.runs_daily` key and its count.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(super) struct RunRow {
    pub(super) day: String,
    pub(super) work_type: String,
    pub(super) fleet_id: Option<Uuid7>,
    pub(super) runs: i64,
}

pub(super) async fn usage_daily(db: &Db, tenant: &Uuid7) -> Vec<DailyRow> {
    sqlx::query_as(SELECT_USAGE_DAILY)
        .bind(tenant)
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

pub(super) async fn ledger_daily(db: &Db, tenant: &Uuid7) -> Vec<DailyRow> {
    sqlx::query_as(SELECT_LEDGER_DAILY)
        .bind(tenant)
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

pub(super) async fn runs_daily(db: &Db, tenant: &Uuid7) -> Vec<RunRow> {
    sqlx::query_as(SELECT_RUNS_DAILY)
        .bind(tenant)
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

pub(super) async fn ledger_runs(db: &Db, tenant: &Uuid7) -> Vec<RunRow> {
    sqlx::query_as(SELECT_LEDGER_RUNS)
        .bind(tenant)
        .bind(LineKind::Runtime.as_str())
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

/// Everything settlement writes for `tenant`: the rollup, the wallet and the ledger.
pub(super) async fn written(db: &Db, tenant: &Uuid7) -> Written {
    Written {
        usage: usage_daily(db, tenant).await,
        runs: runs_daily(db, tenant).await,
        balance: balance(db, tenant).await,
        ledger: ledger(db, tenant).await,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Written {
    pub(super) usage: Vec<DailyRow>,
    pub(super) runs: Vec<RunRow>,
    pub(super) balance: i64,
    pub(super) ledger: (i64, i64, i64),
}
