//! Seeds unsettled ledger lines and settles them through the production biller,
//! so every rollup row a report reads was written by the settle transaction.

use af_core::id::Uuid7;
use af_wire::usage::LineKind;
use afd_billing::biller::{Biller as _, Receipt, Settlement, WorkKey};
use afd_billing::ledger::{self, LedgerBiller};
use afd_billing::price::RatedLine;
use afd_billing::report::{Aggregate, Filter, Report, Request, Window, dimension};
use afd_billing::sql::{posture, work};
use afd_billing::tenant::Billing;
use afd_db::test_util::{TenantSeed, TestDatabase, WalletSeed, mint_id};
use afd_db::{Db, DbRole};
use jiff::Timestamp;

pub(super) const MODEL_A: &str = "report-model-a";
pub(super) const MODEL_B: &str = "report-model-b";
pub(super) const FLEET_NAME: &str = "Payment alerts";
pub(super) const AT: &str = "2026-10-25T12:00:00Z";
pub(super) const FROM: &str = "2026-10-01T00:00:00Z";
pub(super) const THROUGH: &str = "2026-10-25T12:01:00Z";
pub(super) const BEFORE: &str = "2026-09-30T23:59:59.999Z";
pub(super) const LATER: &str = "2026-10-25T12:02:00Z";
pub(super) const INPUT: i64 = 23;
pub(super) const OUTPUT: i64 = 7;
pub(super) const RUNTIME: i64 = 31;
/// What each fixture wallet opens with; settlements debit it.
pub(super) const BALANCE: i64 = 1_000_000_000;

/// Writes lines as a work's own transaction logs them: unsettled.
const INSERT_LINES: &str = "\
INSERT INTO billing.usage_lines (
    id, work_type, work_id, seq, kind, tenant_id, workspace_id,
    fleet_id, fleet_name, event_id, authoring_attempt_id, provider, model, posture,
    requested_at, input_token_count, cached_input_token_count,
    cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
    output_token_count, runtime_ms, unpriced_token_count, charged_nanos,
    settled_at, created_at, updated_at
)
SELECT id, work_kind, work_id, seq, kind, tenant, tenant,
       fleet, name, CASE WHEN fleet IS NOT NULL THEN work_id END,
       CASE WHEN fleet IS NULL THEN work_id END,
       model, model, posture, at::timestamptz, $14, 0, 0, 0, 0, $15, $16,
       unpriced, money, NULL, at::timestamptz, at::timestamptz
  FROM unnest($1, $2::text[], $3, $4::bigint[], $5::text[],
              $6, $7, $8::text[], $9::text[], $10::text[],
              $11::text[], $12::bigint[], $13::bigint[])
    AS source(id, work_kind, work_id, seq, kind, tenant, fleet, name, model, posture, at, money, unpriced)
";

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

pub(super) struct Fixture {
    pub(super) admin: Db,
    pub(super) api: Db,
    pub(super) billing: Billing,
    pub(super) biller: LedgerBiller,
    pub(super) tenant: Uuid7,
    pub(super) foreign: Uuid7,
    pub(super) fleet: Uuid7,
}

impl Fixture {
    pub(super) async fn shared() -> Self {
        Self::open(&TestDatabase::shared()).await
    }

    /// Two funded tenants: the rollup's foreign key needs each to exist.
    pub(super) async fn open(database: &TestDatabase) -> Self {
        let admin = database.open(DbRole::Migrator, &[]).await;
        let api = database.open(DbRole::Api, &[]).await;
        let fixture = Self {
            billing: Billing::new(api.clone()),
            biller: LedgerBiller::on_system_clock(api.clone()),
            admin,
            api,
            tenant: id(),
            foreign: id(),
            fleet: id(),
        };
        for tenant in [&fixture.tenant, &fixture.foreign] {
            fund(&fixture.admin, tenant).await;
        }
        fixture
    }

    pub(super) async fn close(self) {
        self.api.close().await;
        self.admin.close().await;
    }
}

/// Seeds `tenant` and its wallet at [`BALANCE`].
pub(super) async fn fund(db: &Db, tenant: &Uuid7) {
    let mut connection = db.acquire().await.unwrap();
    TenantSeed::new(tenant)
        .insert(&mut *connection)
        .await
        .unwrap();
    WalletSeed::new(tenant, BALANCE)
        .insert(&mut *connection)
        .await
        .unwrap();
}

#[derive(Debug, Clone)]
pub(super) struct Line {
    pub(super) id: Uuid7,
    pub(super) work_type: &'static str,
    pub(super) work_id: Uuid7,
    pub(super) kind: LineKind,
    pub(super) sequence: i64,
    pub(super) tenant: Uuid7,
    pub(super) fleet: Option<Uuid7>,
    pub(super) name: Option<String>,
    pub(super) model: &'static str,
    pub(super) posture: &'static str,
    pub(super) at: &'static str,
    pub(super) money: i64,
    pub(super) unpriced: i64,
}

impl Line {
    /// The first runtime slice of a new fleet run.
    pub(super) fn runtime(tenant: &Uuid7, fleet: &Uuid7, money: i64) -> Self {
        Self {
            id: mint_id(),
            work_type: work::FLEET_LEASE,
            work_id: mint_id(),
            kind: LineKind::Runtime,
            sequence: 0,
            tenant: tenant.clone(),
            fleet: Some(fleet.clone()),
            name: Some(FLEET_NAME.to_owned()),
            model: MODEL_A,
            posture: posture::PLATFORM,
            at: AT,
            money,
            unpriced: 0,
        }
    }

    /// The first model call of a new setup attempt.
    pub(super) fn setup(tenant: &Uuid7, money: i64) -> Self {
        let mut line = Self::runtime(tenant, &id(), money);
        line.work_type = work::AUTHORING_ATTEMPT;
        line.kind = LineKind::Model;
        line.fleet = None;
        line.name = None;
        line.model = MODEL_B;
        line
    }

    /// Another line of this line's work, of `kind` at `sequence`.
    pub(super) fn next(&self, kind: LineKind, sequence: i64, money: i64) -> Self {
        Self {
            id: mint_id(),
            kind,
            sequence,
            money,
            ..self.clone()
        }
    }

    pub(super) fn work(&self) -> WorkKey {
        if self.work_type == work::FLEET_LEASE {
            WorkKey::FleetLease(self.work_id.clone())
        } else {
            WorkKey::AuthoringAttempt(self.work_id.clone())
        }
    }
}

pub(super) fn id() -> Uuid7 {
    mint_id()
}

pub(super) fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

pub(super) fn request() -> Request {
    Request {
        window: Window {
            from: instant(FROM),
            through: instant(THROUGH),
        },
        filter: Filter::default(),
    }
}

pub(super) fn summary(report: &Report) -> &Aggregate {
    report
        .aggregates
        .iter()
        .find(|row| row.dimension == dimension::SUMMARY)
        .unwrap()
}

/// The `dimension` group keyed `key`; `None` is the setup group or a total.
pub(super) fn group<'a>(report: &'a Report, dimension: i32, key: Option<&str>) -> &'a Aggregate {
    let found = report
        .aggregates
        .iter()
        .find(|row| row.dimension == dimension && row.key.as_deref() == key);
    assert!(
        found.is_some(),
        "no group {dimension}/{key:?} in {:?}",
        report.aggregates
    );
    found.unwrap()
}

/// Logs `lines` unsettled, as the work's own transaction does.
pub(super) async fn seed(db: &Db, lines: &[Line]) {
    let column = |field: fn(&Line) -> String| lines.iter().map(field).collect::<Vec<_>>();
    sqlx::query(INSERT_LINES)
        .bind(lines.iter().map(|line| line.id.clone()).collect::<Vec<_>>())
        .bind(column(|line| line.work_type.to_owned()))
        .bind(
            lines
                .iter()
                .map(|line| line.work_id.clone())
                .collect::<Vec<_>>(),
        )
        .bind(lines.iter().map(|line| line.sequence).collect::<Vec<_>>())
        .bind(column(|line| line.kind.as_str().to_owned()))
        .bind(
            lines
                .iter()
                .map(|line| line.tenant.clone())
                .collect::<Vec<_>>(),
        )
        .bind(
            lines
                .iter()
                .map(|line| line.fleet.clone())
                .collect::<Vec<_>>(),
        )
        .bind(
            lines
                .iter()
                .map(|line| line.name.as_deref())
                .collect::<Vec<_>>(),
        )
        .bind(column(|line| line.model.to_owned()))
        .bind(column(|line| line.posture.to_owned()))
        .bind(column(|line| line.at.to_owned()))
        .bind(lines.iter().map(|line| line.money).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.unpriced).collect::<Vec<_>>())
        .bind(INPUT)
        .bind(OUTPUT)
        .bind(RUNTIME)
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .unwrap();
}

/// `work`'s unsettled lines as the relay groups them for the biller.
pub(super) async fn pending(db: &Db, work: &WorkKey) -> Vec<Settlement> {
    ledger::unsettled_work(&mut *db.acquire().await.unwrap(), work)
        .await
        .unwrap()
}

/// Settles every unsettled line of `work` through the production biller.
/// Answers the lines settled and the nanos charged, summed over its groups.
pub(super) async fn settle(biller: &LedgerBiller, db: &Db, work: &WorkKey) -> (usize, i64) {
    let mut total = (0, 0);
    for settlement in pending(db, work).await {
        let receipt: Receipt = biller.settle(&settlement).await.unwrap();
        total.0 += receipt.settled_lines;
        total.1 += receipt.charged.as_i64();
    }
    total
}

/// Settles only the unsettled lines of `work` that `keep` selects: one relay
/// batch of a work whose other lines a later batch settles.
pub(super) async fn settle_only(
    biller: &LedgerBiller,
    db: &Db,
    work: &WorkKey,
    keep: impl Fn(&RatedLine) -> bool,
) -> (usize, i64) {
    let mut total = (0, 0);
    for mut settlement in pending(db, work).await {
        settlement.lines.retain(&keep);
        if settlement.lines.is_empty() {
            continue;
        }
        let receipt = biller.settle(&settlement).await.unwrap();
        total.0 += receipt.settled_lines;
        total.1 += receipt.charged.as_i64();
    }
    total
}

/// Whether `rated` is `line`, by the key settlement matches on.
pub(super) fn is(rated: &RatedLine, line: &Line) -> bool {
    rated.usage.kind == line.kind && i64::from(rated.usage.seq) == line.sequence
}

/// Seeds `lines` and settles each of their works, as the relay would.
pub(super) async fn record(f: &Fixture, lines: &[Line]) {
    seed(&f.admin, lines).await;
    let mut works: Vec<WorkKey> = Vec::new();
    for work in lines.iter().map(Line::work) {
        if !works.contains(&work) {
            works.push(work);
        }
    }
    for work in works {
        settle(&f.biller, &f.admin, &work).await;
    }
}

/// Includes pending rows, so a report that mutates money or settlement fails the check.
pub(super) async fn ledger(db: &Db, tenant: &Uuid7) -> (i64, i64, i64) {
    sqlx::query_as("SELECT COUNT(*)::bigint, COALESCE(SUM(charged_nanos),0)::bigint, COUNT(settled_at)::bigint FROM billing.usage_lines WHERE tenant_id=$1")
        .bind(tenant).fetch_one(&mut *db.acquire().await.unwrap()).await.unwrap()
}

pub(super) async fn balance(db: &Db, tenant: &Uuid7) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1")
        .bind(tenant)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .unwrap()
}

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
