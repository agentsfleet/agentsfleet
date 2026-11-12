//! Unsettled usage lines for the billing tests: an orphaned line, and the
//! settlement a held lease's line makes.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_wire::usage::{LineKind, UsageLine};
use afd_billing::Posture;
use afd_billing::biller::{Account, Attribution, Settlement, WorkKey};
use afd_billing::ledger::log_lines;
use afd_billing::price::RatedLine;
use afd_fleet::lease::billing_sweep::GRACE;

use crate::integration_billing_sweep::{AGED, LINE, RUNTIME_MS};
use crate::report_seed::Held;

/// The state a crash leaves: one line logged, then the process died before
/// any relay pass settled it, and the lease expired unreported, all longer ago
/// than the grace.
pub(crate) async fn orphan(held: &Held) {
    let database = &held.fixtures.database;
    let settlement = settlement(held).await;
    log_lines(
        &mut *database.acquire().await.expect("a pooled connection"),
        &Entropy::new(),
        held.now,
        &settlement,
    )
    .await
    .expect("the line is logged");

    let mut connection = database.acquire().await.expect("a pooled connection");
    sqlx::query("UPDATE runner.leases SET status = $2 WHERE id = $1")
        .bind(&held.lease_key)
        .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
        .execute(&mut *connection)
        .await
        .expect("the lease ends unreported");
    for (table, key) in [("billing.usage_lines", "work_id"), ("runner.leases", "id")] {
        sqlx::query(sqlx::AssertSqlSafe(
            AGED.replace("{table}", table).replace("{key}", key),
        ))
        .bind(&held.lease_key)
        .bind(GRACE.saturating_mul(2))
        .execute(&mut *connection)
        .await
        .expect("the rows age past the grace");
    }
}

/// One runtime-only line of `held`'s lease, attributed to its fleet.
pub(crate) async fn settlement(held: &Held) -> Settlement {
    let workspace: Uuid7 =
        sqlx::query_scalar("SELECT workspace_id FROM fleet.fleets WHERE id = $1")
            .bind(&held.fleet)
            .fetch_one(
                &mut *held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("a pooled connection"),
            )
            .await
            .expect("the fixture fleet is readable");
    Settlement {
        work: WorkKey::FleetLease(held.lease_key.clone()),
        payer: Account {
            tenant: held.tenant.clone(),
            workspace,
        },
        attribution: Attribution::Fleet {
            fleet: held.fleet.clone(),
            fleet_name: held.fleet.clone().into(),
            event_id: held.event_id.clone(),
        },
        provider: crate::seed::PROVIDER.to_owned(),
        model: crate::seed::MODEL.to_owned(),
        posture: Posture::Platform,
        lines: vec![RatedLine {
            usage: UsageLine {
                kind: LineKind::Runtime,
                seq: 0,
                at: held.now,
                input_tokens: 0,
                cached_input_tokens: 0,
                cache_write_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
                output_tokens: 0,
                runtime_ms: RUNTIME_MS,
            },
            price: None,
            charged: LINE,
            unpriced_tokens: 0,
        }],
    }
}
