//! One lease retains independent model headers across lost settlements.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use std::{panic::AssertUnwindSafe, time::Duration};

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_billing::{
    Nanos, Posture,
    biller::{Account, Attribution, Biller as _, Settlement, WorkKey, conformance::BALANCE},
    ledger::{LedgerBiller, Resettled, log_lines, resettle, unsettled},
};
use afd_db::{
    Db, Migrator,
    config::DbRole,
    test_util::{
        FleetSeed, TenantSeed, TestDatabase, WalletSeed, WorkspaceSeed, mint_id, mint_uuid7,
    },
};
use futures_util::FutureExt as _;
use jiff::Timestamp;

/// Whole milliseconds survive the ledger's TIMESTAMPTZ(3) round trip.
const AT: Timestamp = Timestamp::constant(1_700_000_000, 123_000_000);
const FLEET_NAME: &str = "Mixed model recovery";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn mixed_model_usage_preserves_headers_and_resettles_each_sequence_once() {
    // A sweeper reads globally. Its fixture owns a database so another test's
    // legitimate sweep cannot consume these rows before their assertions.
    let lane = TestDatabase::create().await;
    let outcome = AssertUnwindSafe(tokio::time::timeout(
        Duration::from_secs(45),
        scenario(&lane),
    ))
    .catch_unwind()
    .await;
    tokio::time::timeout(Duration::from_secs(10), lane.cleanup())
        .await
        .expect("owned database cleanup is bounded, including after assertion failure");
    match outcome {
        Ok(result) => result.expect("the entire recovery scenario is bounded"),
        Err(failure) => std::panic::resume_unwind(failure),
    }
}

async fn scenario(lane: &TestDatabase) {
    let migrator = lane.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&migrator)
        .await
        .expect("isolated schema migrates");
    migrator.close().await;
    let database = lane.open(DbRole::Api, &[]).await;
    let payer = Account {
        tenant: mint_uuid7(),
        workspace: mint_uuid7(),
    };
    let fleet = mint_uuid7();
    seed(&database, &payer, &fleet).await;
    recovery(&database, &payer, &fleet).await;
    database.close().await;
}

async fn seed(database: &Db, payer: &Account, fleet: &Uuid7) {
    let mut connection = database.acquire().await.expect("seed connection");
    TenantSeed::new(payer.tenant.as_str())
        .insert(&mut *connection)
        .await
        .expect("tenant seed");
    WorkspaceSeed::new(payer.workspace.as_str(), payer.tenant.as_str())
        .insert(&mut *connection)
        .await
        .expect("workspace seed");
    FleetSeed::new(
        fleet.as_str(),
        payer.workspace.as_str(),
        payer.tenant.as_str(),
        "active",
    )
    .name(FLEET_NAME)
    .insert(&mut *connection)
    .await
    .expect("fleet seed");
    WalletSeed::new(payer.tenant.as_str(), BALANCE.as_i64())
        .insert(&mut *connection)
        .await
        .expect("funded wallet");
}

async fn recovery(database: &Db, payer: &Account, fleet: &Uuid7) {
    let mut expected = groups(payer, fleet);
    reconstruct(database, payer, &mut expected).await;
    let biller = LedgerBiller::on_system_clock(database.clone());
    assert_eq!(
        biller
            .balance(payer)
            .await
            .expect("unsettled balance")
            .balance,
        BALANCE,
        "logged usage has not charged the wallet yet"
    );
    assert_eq!(
        resettle(database, &biller, None)
            .await
            .expect("native recovery"),
        Resettled {
            settled: 5,
            failed: 0,
            ..Resettled::default()
        }
    );
    let after = biller.balance(payer).await.expect("settled balance");
    assert_eq!(
        after.balance.as_i64(),
        BALANCE.as_i64() - 87_000,
        "six unique lines across two leases charge once"
    );
    assert_eq!(
        resettle(database, &biller, None)
            .await
            .expect("repeated recovery"),
        Resettled::default()
    );
    assert_eq!(
        biller.balance(payer).await.expect("replayed balance"),
        after
    );
    let mut connection = database.acquire().await.expect("remaining work read");
    assert_eq!(
        unsettled(&mut *connection, 100, None)
            .await
            .expect("settlement completion read"),
        (Vec::<Settlement>::new(), None)
    );
}

async fn reconstruct(database: &Db, payer: &Account, expected: &mut [Settlement; 5]) {
    let later = expected[0].lines.remove(0);
    let entropy = Entropy::new();
    {
        let mut connection = database.acquire().await.expect("log connection");
        for group in expected.iter() {
            log_lines(&mut *connection, &entropy, AT, group)
                .await
                .expect("usage commits without request-path settlement");
        }
        expected[0].lines.insert(0, later);
        // Sequence 8 arrived first; 2 follows the other models. Replaying 8
        // must not add another charge, and readback must order the group [2, 8].
        log_lines(&mut *connection, &entropy, AT, &expected[0])
            .await
            .expect("later usage and identical replay commit");
        let aged = sqlx::query(include_str!("fixtures/age_mixed_settlements.sql"))
            .bind(payer.tenant.as_str())
            .bind(Duration::from_secs(1))
            .bind(expected[0].work.id().as_str())
            .bind(8_i64)
            .execute(&mut *connection)
            .await
            .expect("owned sequence 8 is strictly older than 2");
        assert_eq!(aged.rows_affected(), 6, "identical replay created no row");
        let (actual, resume) = unsettled(&mut *connection, 100, None)
            .await
            .expect("all stored settlement headers reconstruct");
        assert!(resume.is_none(), "the complete page ends this lap");
        assert_eq!(actual.len(), 5, "four model groups plus another lease");
        for group in expected.iter() {
            assert!(
                actual.contains(group),
                "complete payer, attribution, model, posture and ordered usage survive: {group:?}"
            );
        }
        assert_eq!(
            logged_total(&actual, &expected[0].work),
            Nanos::from_i64(62_000)
        );
        assert_eq!(
            logged_total(&actual, &expected[4].work),
            Nanos::from_i64(25_000)
        );
    }
}

fn groups(payer: &Account, fleet: &Uuid7) -> [Settlement; 5] {
    let work = WorkKey::FleetLease(mint_uuid7());
    let attribution = Attribution::Fleet {
        fleet: fleet.clone(),
        fleet_name: FLEET_NAME.to_owned(),
        event_id: mint_id(),
    };
    // All partitions retain one immutable owner/event header. The other work
    // deliberately uses the same model, proving grouping retains work identity.
    let group = |provider: &str, model: &str, posture, lines| Settlement {
        work: work.clone(),
        payer: payer.clone(),
        attribution: attribution.clone(),
        provider: provider.to_owned(),
        model: model.to_owned(),
        posture,
        lines,
    };
    let mut other = group(
        "openai",
        "coding-model",
        Posture::Platform,
        vec![line(0, 25_000)],
    );
    other.work = WorkKey::FleetLease(mint_uuid7());
    [
        group(
            "openai",
            "coding-model",
            Posture::Platform,
            vec![line(2, 25_000), line(8, 25_000)],
        ),
        group(
            "openai",
            "decision-model",
            Posture::Platform,
            vec![line(6, 9_000)],
        ),
        group(
            "openai",
            "decision-model",
            Posture::SelfManaged,
            vec![line(4, 0)],
        ),
        group(
            "typesafe",
            "decision-model",
            Posture::Platform,
            vec![line(1, 3_000)],
        ),
        other,
    ]
}

fn line(seq: u32, charged: i64) -> afd_billing::price::RatedLine {
    // The existing recorded-charge fixture supplies normalized usage. This
    // scenario reconstructs saved charges; it does not evaluate model rates.
    let mut line = crate::biller::rated(seq);
    line.usage.at = AT;
    line.charged = Nanos::from_i64(charged);
    line
}

fn logged_total(settlements: &[Settlement], work: &WorkKey) -> Nanos {
    Nanos::from_i64(
        settlements
            .iter()
            .filter(|group| &group.work == work)
            .flat_map(|group| &group.lines)
            .map(|line| line.charged.as_i64())
            .sum(),
    )
}
