//! A runtime renewal's metered slice and an authoring settlement debit one wallet.
//!
//! Both kinds run on one two-worker host for one tenant. The wallet row is held
//! while both requests queue behind it, in each order; on release both commit,
//! neither debit is lost, and neither waits forever on the other's locks.

use super::{
    fixture::Running,
    replicas::waiting,
    seat::{FUNDED, Seat},
};
use crate::rows::Row;
use afd_authoring::Claimed;
use afd_billing::{Cumulative, Meter, SliceRates};
use afd_core::{clock, id::Uuid7};
use afd_db::test_util::mint_id;
use afd_fleet::lease::Renewed;
use afd_wire::authoring::{Accounting, Knowledge, Outcome, Report, Reported};
use sqlx::Connection as _;
use std::time::Duration;

const DEADLOCK_BOUND: Duration = Duration::from_secs(30);
/// One million input tokens at a thousand nanos per million: a fixed slice,
/// independent of how long the renewal waited.
const FLEET_SLICE: i64 = 1_000;
const INPUT_TOKENS: i64 = 1_000_000;
/// One reported millisecond at the runtime rate.
const AUTHORING_DEBIT: i64 = 100;
const HOLD_WALLET: &str =
    "SELECT pg_backend_pid() FROM billing.wallets WHERE tenant_id = $1::uuid FOR UPDATE";
const BALANCE: &str = "SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1::uuid";

#[derive(Debug, Clone, Copy)]
enum First {
    Renewal,
    Settlement,
}

/// How far the authoring turn is backdated so its attempt reads as already started.
const AGE: Duration = Duration::from_secs(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_runtime_debit_and_authoring_settlement_interleave_on_one_wallet() {
    for first in [First::Renewal, First::Settlement] {
        let finished = Box::pin(tokio::time::timeout(DEADLOCK_BOUND, scenario(first))).await;
        assert!(finished.is_ok(), "{first:?} deadlocked past the bound");
    }
}

async fn scenario(first: First) {
    let running = Running::create().await;
    let database = &running.fixtures.database;
    sqlx::query("UPDATE runner.runners SET worker_count = 2 WHERE id = $1::uuid")
        .bind(running.runner.as_str())
        .execute(&mut *database.acquire().await.expect("widen"))
        .await
        .expect("a second worker for the authoring turn");
    let seat = Seat::create(&running).await;
    let drafts = Seat::drafts(database);
    let draft = seat.queue(&drafts).await;
    let lease = Seat::admit(&drafts, &running.runner, &draft)
        .await
        .expect("the second worker admits the turn beside the runtime lease");
    running
        .fixtures
        .age(Row::Attempt(lease.attempt_id.as_str()), "created_at", AGE)
        .await;
    assert_eq!(balance(&running, &seat.tenant).await, FUNDED);

    let leases = running.fixtures.leases();
    let renew = leases.extend(
        running.lease.as_str(),
        &running.runner,
        metered(),
        clock::now(),
    );
    let authoring = lease_id(&lease);
    let settle = drafts.report(&running.runner, &authoring, accounting(&lease));
    let (renewed, reported) = match first {
        First::Renewal => Box::pin(queued(&running, &seat.tenant, renew, settle)).await,
        First::Settlement => {
            let (reported, renewed) = Box::pin(queued(&running, &seat.tenant, settle, renew)).await;
            (renewed, reported)
        }
    };
    let renewed = renewed.expect("the renewal is a verdict");
    assert!(
        matches!(renewed, Renewed::Extended { charged, .. } if charged.as_i64() == FLEET_SLICE),
        "{first:?}: the live lease renews and is charged the token slice, got {renewed:?}"
    );
    let reported: Reported = reported.expect("the settlement commits");
    assert!(reported.settled && !reported.replayed, "{first:?}");
    assert_eq!(
        balance(&running, &seat.tenant).await,
        FUNDED - FLEET_SLICE - AUTHORING_DEBIT,
        "{first:?}: both debits land; neither overwrote the other"
    );
    seat.remove(database).await;
    running.cleanup().await;
}

/// Hold the wallet row, queue `first` then `second` behind it, release both.
async fn queued<A, B>(
    running: &Running,
    tenant: &str,
    first: impl Future<Output = A>,
    second: impl Future<Output = B>,
) -> (A, B) {
    let mut connection = running.fixtures.database.acquire().await.expect("holder");
    let mut held = connection.begin().await.expect("open wallet blocker");
    let blocker: i32 = sqlx::query_scalar(HOLD_WALLET)
        .bind(tenant)
        .fetch_one(&mut *held)
        .await
        .expect("hold the tenant wallet row");
    let later = async {
        waiting(running, blocker, 1).await;
        second.await
    };
    let release = async {
        waiting(running, blocker, 2).await;
        held.commit().await.expect("release the wallet to both");
    };
    let (first, second, ()) = tokio::join!(first, later, release);
    drop(connection);
    (first, second)
}

fn metered() -> Meter {
    Meter {
        cumulative: Cumulative {
            input: INPUT_TOKENS,
            cached: 0,
            output: 0,
        },
        rates: SliceRates {
            run_nanos_per_sec: 0,
            input_nanos_per_mtok: FLEET_SLICE,
            cached_input_nanos_per_mtok: 0,
            output_nanos_per_mtok: 0,
        },
    }
}

fn accounting(lease: &Claimed) -> Report {
    Report::Accounting {
        accounting: Accounting {
            fence: lease.fence.clone(),
            deletion_generation: lease.deletion_generation.clone(),
            report_id: mint_id(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: afd_wire::authoring::Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
    }
}

fn lease_id(lease: &Claimed) -> Uuid7 {
    Uuid7::parse(&lease.lease_id).expect("the lease names a UUIDv7")
}

async fn balance(running: &Running, tenant: &str) -> i64 {
    sqlx::query_scalar(BALANCE)
        .bind(tenant)
        .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
        .await
        .expect("the tenant wallet")
}
