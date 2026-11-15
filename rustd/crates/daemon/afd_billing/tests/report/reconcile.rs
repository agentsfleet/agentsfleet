//! The rollup against the ledger it summarises: after a month of runs settled
//! in whole, split and repeated passes, every rollup key and every report
//! group equals what the settled ledger lines sum to.

use super::fixture::{self, Fixture, Line};
use af_wire::usage::LineKind;
use afd_billing::biller::Biller as _;
use afd_billing::report::{Aggregate, Filter, dimension};
use afd_billing::sql::work;
use jiff_sqlx::ToSqlx as _;

const FLEET_NAMES: [&str; 3] = ["Payment alerts", "Release notes", "Support triage"];
/// Runs in the seeded month; every fourth belongs to the other tenant.
const RUNS: usize = 12;

/// The report's groups, computed from the settled ledger lines instead of the
/// rollup: the same whole-UTC-day period, filters and run rule.
const LEDGER_REPORT: &str = "\
WITH lines AS (
    SELECT fleet_id, fleet_name, model, (created_at AT TIME ZONE 'UTC')::date AS day,
           charged_nanos, input_token_count, output_token_count, runtime_ms, unpriced_token_count,
           ((fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)) AS selected
      FROM billing.usage_lines
     WHERE tenant_id = $1 AND settled_at IS NOT NULL
       AND (created_at AT TIME ZONE 'UTC')::date
           BETWEEN ($2::timestamptz AT TIME ZONE 'UTC')::date AND ($3::timestamptz AT TIME ZONE 'UTC')::date
), first_slices AS (
    SELECT DISTINCT ON (work_type, work_id) work_type, fleet_id, created_at, settled_at
      FROM billing.usage_lines WHERE tenant_id = $1 AND kind = $6
     ORDER BY work_type, work_id, seq
), runs AS (
    SELECT fleet_id, COUNT(*) AS runs FROM first_slices
     WHERE settled_at IS NOT NULL
       AND (created_at AT TIME ZONE 'UTC')::date
           BETWEEN ($2::timestamptz AT TIME ZONE 'UTC')::date AND ($3::timestamptz AT TIME ZONE 'UTC')::date
       AND (fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)
     GROUP BY fleet_id
)
SELECT $7::int AS dimension, NULL::text AS key, NULL::text AS label, NULL::text AS day,
       ARRAY[]::text[] AS models,
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint AS charged_nanos,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint AS input_tokens,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint AS output_tokens,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint AS runtime_ms,
       COUNT(*) FILTER (WHERE selected)::bigint AS line_count,
       (SELECT COALESCE(SUM(runs), 0) FROM runs)::bigint AS activity_count,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint AS unpriced_tokens
  FROM lines
UNION ALL
SELECT $8, l.fleet_id::text, MAX(l.fleet_name), NULL,
       COALESCE(ARRAY_AGG(DISTINCT l.model ORDER BY l.model) FILTER (WHERE l.selected AND l.model <> ''),
                ARRAY[]::text[]),
       COALESCE(SUM(l.charged_nanos) FILTER (WHERE l.selected), 0)::bigint,
       COALESCE(SUM(l.input_token_count) FILTER (WHERE l.selected), 0)::bigint,
       COALESCE(SUM(l.output_token_count) FILTER (WHERE l.selected), 0)::bigint,
       COALESCE(SUM(l.runtime_ms) FILTER (WHERE l.selected), 0)::bigint,
       COUNT(*) FILTER (WHERE l.selected)::bigint,
       COALESCE(MAX(r.runs), 0)::bigint,
       COALESCE(SUM(l.unpriced_token_count) FILTER (WHERE l.selected), 0)::bigint
  FROM lines l LEFT JOIN runs r ON r.fleet_id IS NOT DISTINCT FROM l.fleet_id
 GROUP BY l.fleet_id
UNION ALL
SELECT $9, model, NULL, NULL, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COUNT(*) FILTER (WHERE selected)::bigint, NULL::bigint,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint
  FROM lines GROUP BY model
UNION ALL
SELECT $10, NULL, NULL, day::text, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COUNT(*) FILTER (WHERE selected)::bigint, NULL::bigint,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint
  FROM lines GROUP BY day";

/// One report group, in a shape both the report and the ledger query fill.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, sqlx::FromRow)]
struct Group {
    dimension: i32,
    key: Option<String>,
    day: Option<String>,
    label: Option<String>,
    models: Vec<String>,
    charged_nanos: i64,
    input_tokens: i64,
    output_tokens: i64,
    runtime_ms: i64,
    line_count: i64,
    activity_count: Option<i64>,
    unpriced_tokens: i64,
}

impl From<&Aggregate> for Group {
    fn from(row: &Aggregate) -> Self {
        Self {
            dimension: row.dimension,
            key: row.key.clone(),
            day: row.day.map(|day| day.to_string()),
            label: row.label.clone(),
            models: row.models.clone(),
            charged_nanos: row.charged_nanos,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            runtime_ms: row.runtime_ms,
            line_count: row.line_count,
            activity_count: row.activity_count,
            unpriced_tokens: row.unpriced_tokens,
        }
    }
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn report_groups_equal_the_settled_ledger_for_a_seeded_month() {
    let f = Fixture::shared().await;
    seed_month(&f).await;
    let request = fixture::request();
    let total = fixture::summary(&f.billing.report(&f.tenant, &request).await.unwrap()).clone();
    // Nine of the twelve runs are this tenant's, each first slice settled.
    assert_eq!(total.activity_count, Some(9));
    for filter in [
        Filter::default(),
        Filter {
            fleet: Some(f.fleet.clone()),
            ..Filter::default()
        },
        Filter {
            work_type: Some(work::AUTHORING_ATTEMPT),
            ..Filter::default()
        },
        Filter {
            work_type: Some(work::FLEET_LEASE),
            ..Filter::default()
        },
    ] {
        let mut request = fixture::request();
        request.filter = filter;
        let report = f.billing.report(&f.tenant, &request).await.unwrap();
        let mut served: Vec<Group> = report.aggregates.iter().map(Group::from).collect();
        let mut expected: Vec<Group> = sqlx::query_as(LEDGER_REPORT)
            .bind(&f.tenant)
            .bind(request.window.from.to_sqlx())
            .bind(request.window.through.to_sqlx())
            .bind(request.filter.fleet.as_ref())
            .bind(request.filter.work_type)
            .bind(LineKind::Runtime.as_str())
            .bind(dimension::SUMMARY)
            .bind(dimension::FLEET)
            .bind(dimension::MODEL)
            .bind(dimension::DAY)
            .fetch_all(&mut *f.admin.acquire().await.unwrap())
            .await
            .unwrap();
        served.sort();
        expected.sort();
        assert_eq!(served, expected, "{:?}", request.filter);
        // Three fleets and setup, whatever the filter selects.
        let fleets = served
            .iter()
            .filter(|group| group.dimension == dimension::FLEET)
            .count();
        assert_eq!(fleets, 4, "{:?}", request.filter);
    }
    f.close().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn the_rollup_equals_the_settled_ledger_key_by_key() {
    let f = Fixture::shared().await;
    seed_month(&f).await;
    // The tenant's nine runs hold 21 settled keys, its four settled setups 4
    // and its runs either side of the period 2; the other tenant's three runs 7.
    for (tenant, keys, runs) in [(&f.tenant, 27, 11), (&f.foreign, 7, 3)] {
        let rollup = fixture::usage_daily(&f.admin, tenant).await;
        assert_eq!(rollup.len(), keys);
        assert_eq!(rollup, fixture::ledger_daily(&f.admin, tenant).await);
        let counted = fixture::runs_daily(&f.admin, tenant).await;
        assert_eq!(counted.len(), runs);
        assert_eq!(counted, fixture::ledger_runs(&f.admin, tenant).await);
    }
    // Lines left pending are what make "settled" the operative word.
    let (lines, _, settled) = fixture::ledger(&f.admin, &f.tenant).await;
    assert_eq!((lines, settled), (43, 36));
    f.close().await;
}

/// Twelve runs over October, each crossing UTC midnight on three fleets: some
/// settled in one batch, some later day first and then repeated, some left
/// half pending. Setup attempts, one pending, and runs on either side of the
/// period complete the month.
async fn seed_month(f: &Fixture) {
    let fleets = [f.fleet.clone(), fixture::id(), fixture::id()];
    for run in 0..RUNS {
        let tenant = if run % 4 == 3 { &f.foreign } else { &f.tenant };
        let money = i64::try_from(run).unwrap();
        let (day, next) = (1 + 2 * run, 2 + 2 * run);
        let mut first = Line::runtime(tenant, fleets.get(run % 3).unwrap(), 10 + money);
        first.name = Some(FLEET_NAMES.get(run % 3).unwrap().to_string());
        first.at = at(format!("2026-10-{day:02}T23:59:59.000Z"));
        let mut call = first.next(LineKind::Model, 0, 100 + money);
        call.at = at(format!("2026-10-{day:02}T23:59:59.500Z"));
        let mut slice = first.next(LineKind::Runtime, 60_000, 20 + money);
        slice.at = at(format!("2026-10-{next:02}T00:00:30.000Z"));
        if run % 3 == 0 {
            slice.model = "";
        }
        let mut later = first.next(LineKind::Model, 1, 200 + money);
        later.model = fixture::MODEL_B;
        later.at = at(format!("2026-10-{next:02}T00:01:00.000Z"));
        fixture::seed(&f.admin, &[first.clone(), call.clone(), slice, later]).await;
        settle_run(f, &first, &call, run % 3).await;
    }
    for attempt in 0..4_i64 {
        let mut setup = Line::setup(&f.tenant, 30 + attempt);
        setup.at = at(format!("2026-10-{:02}T08:00:00.000Z", 5 + 5 * attempt));
        setup.unpriced = attempt;
        fixture::record(f, &[setup]).await;
    }
    fixture::seed(&f.admin, &[Line::setup(&f.tenant, 77)]).await;
    let mut before = Line::runtime(&f.tenant, &f.fleet, 88);
    before.at = fixture::BEFORE;
    let mut after = Line::runtime(&f.tenant, &f.fleet, 99);
    after.at = "2026-10-26T00:00:00Z";
    fixture::record(f, &[before, after]).await;
}

/// Settles one run's lines in the pass shape `mode` names.
async fn settle_run(f: &Fixture, first: &Line, call: &Line, mode: usize) {
    let work = first.work();
    let day_one = |rated: &afd_billing::price::RatedLine| {
        fixture::is(rated, first) || fixture::is(rated, call)
    };
    match mode {
        0 => {
            fixture::settle(&f.biller, &f.admin, &work).await;
        }
        1 => {
            let originals = fixture::pending(&f.admin, &work).await;
            fixture::settle_only(&f.biller, &f.admin, &work, |rated| !day_one(rated)).await;
            fixture::settle(&f.biller, &f.admin, &work).await;
            for settlement in &originals {
                let repeat = f.biller.settle(settlement).await.unwrap();
                assert_eq!(repeat.settled_lines, 0, "a repeated pass settles nothing");
            }
        }
        _ => {
            fixture::settle_only(&f.biller, &f.admin, &work, day_one).await;
        }
    }
}

/// A seeded instant; the fixture's lines hold their instants for the test's life.
fn at(text: String) -> &'static str {
    text.leak()
}
