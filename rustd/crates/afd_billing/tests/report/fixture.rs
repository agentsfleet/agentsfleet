use afd_billing::report::{Filter, Request, Window, dimension};
use afd_billing::sql::{posture, work};
use afd_billing::tenant::Billing;
use afd_core::id::Uuid7;
use afd_db::test_util::{TestDatabase, mint_id};
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

const INSERT_LINES: &str = "\
INSERT INTO billing.usage_lines (
    id, work_type, work_id, seq, tenant_id, workspace_id,
    fleet_id, fleet_name, authoring_attempt_id, provider, model, posture,
    requested_at, input_token_count, cached_input_token_count,
    cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
    output_token_count, runtime_ms, unpriced_token_count, charged_nanos,
    settled_at, created_at, updated_at
)
SELECT id::uuid, kind, work_id::uuid, seq, tenant::uuid, tenant::uuid,
       fleet::uuid, name, CASE WHEN fleet IS NULL THEN work_id::uuid END,
       model, model, posture, at::timestamptz, $14, 0, 0, 0, 0, $15, $16,
       unpriced, money, settled::timestamptz, at::timestamptz, at::timestamptz
  FROM unnest($1::text[], $2::text[], $3::text[], $4::bigint[], $5::text[],
              $6::text[], $7::text[], $8::text[], $9::text[], $10::text[],
              $11::text[], $12::bigint[], $13::bigint[])
    AS source(id, kind, work_id, seq, tenant, fleet, name, model, posture, at, settled, money, unpriced)
";

pub(super) struct Fixture {
    pub(super) admin: Db,
    pub(super) api: Db,
    pub(super) billing: Billing,
    pub(super) tenant: Uuid7,
    pub(super) foreign: Uuid7,
    pub(super) fleet: Uuid7,
}

impl Fixture {
    pub(super) async fn shared() -> Self {
        Self::open(&TestDatabase::shared()).await
    }

    pub(super) async fn open(database: &TestDatabase) -> Self {
        let admin = database.open(DbRole::Migrator, &[]).await;
        let api = database.open(DbRole::Api, &[]).await;
        Self {
            admin,
            billing: Billing::new(api.clone()),
            api,
            tenant: id(),
            foreign: id(),
            fleet: id(),
        }
    }

    pub(super) async fn close(self) {
        self.api.close().await;
        self.admin.close().await;
    }
}

pub(super) struct Line {
    pub(super) id: String,
    pub(super) work_type: &'static str,
    pub(super) work_id: String,
    pub(super) sequence: i64,
    pub(super) tenant: String,
    pub(super) fleet: Option<String>,
    pub(super) name: Option<String>,
    pub(super) model: &'static str,
    pub(super) posture: &'static str,
    pub(super) at: &'static str,
    pub(super) settled: Option<&'static str>,
    pub(super) money: i64,
    pub(super) unpriced: i64,
}

impl Line {
    pub(super) fn runtime(tenant: &Uuid7, fleet: &Uuid7, money: i64) -> Self {
        Self {
            id: mint_id(),
            work_type: work::FLEET_LEASE,
            work_id: mint_id(),
            sequence: 0,
            tenant: tenant.as_str().to_owned(),
            fleet: Some(fleet.as_str().to_owned()),
            name: Some(FLEET_NAME.to_owned()),
            model: MODEL_A,
            posture: posture::PLATFORM,
            at: AT,
            settled: Some(AT),
            money,
            unpriced: 0,
        }
    }

    pub(super) fn setup(tenant: &Uuid7, money: i64) -> Self {
        let mut line = Self::runtime(tenant, &id(), money);
        line.work_type = work::AUTHORING_ATTEMPT;
        line.fleet = None;
        line.name = None;
        line.model = MODEL_B;
        line
    }
}

pub(super) fn id() -> Uuid7 {
    Uuid7::parse(&mint_id()).unwrap()
}

pub(super) fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

pub(super) fn request(limit: u32) -> Request {
    Request {
        window: Window {
            from: instant(FROM),
            through: instant(THROUGH),
        },
        filter: Filter::default(),
        limit,
        after: None,
    }
}

pub(super) fn summary(report: &afd_billing::report::Report) -> &afd_billing::report::Aggregate {
    report
        .aggregates
        .iter()
        .find(|row| row.dimension == dimension::SUMMARY)
        .unwrap()
}

pub(super) async fn seed(db: &Db, lines: &[Line]) {
    sqlx::query(INSERT_LINES)
        .bind(
            lines
                .iter()
                .map(|line| line.id.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(lines.iter().map(|line| line.work_type).collect::<Vec<_>>())
        .bind(
            lines
                .iter()
                .map(|line| line.work_id.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(lines.iter().map(|line| line.sequence).collect::<Vec<_>>())
        .bind(
            lines
                .iter()
                .map(|line| line.tenant.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(
            lines
                .iter()
                .map(|line| line.fleet.as_deref())
                .collect::<Vec<_>>(),
        )
        .bind(
            lines
                .iter()
                .map(|line| line.name.as_deref())
                .collect::<Vec<_>>(),
        )
        .bind(lines.iter().map(|line| line.model).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.posture).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.at).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.settled).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.money).collect::<Vec<_>>())
        .bind(lines.iter().map(|line| line.unpriced).collect::<Vec<_>>())
        .bind(INPUT)
        .bind(OUTPUT)
        .bind(RUNTIME)
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .unwrap();
}

/// Includes pending rows, so a report that mutates money or settlement fails the check.
pub(super) async fn ledger(db: &Db, tenant: &Uuid7) -> (i64, i64, i64) {
    sqlx::query_as("SELECT COUNT(*)::bigint, COALESCE(SUM(charged_nanos),0)::bigint, COUNT(settled_at)::bigint FROM billing.usage_lines WHERE tenant_id=$1::uuid")
        .bind(tenant.as_str()).fetch_one(&mut *db.acquire().await.unwrap()).await.unwrap()
}
