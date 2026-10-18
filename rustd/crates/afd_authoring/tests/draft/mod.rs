//! Every database proof owns its people, workspace, models and private rows.

#![expect(
    clippy::expect_used,
    reason = "fixture preconditions fail the proof loudly"
)]

mod admission;
mod attempt;
mod cleanup;
mod contention;
mod creation;
mod edits;
mod fixture;
mod funnel;
mod library;
mod listing;
mod maintenance;
mod messages;
mod model_calls;
mod model_metadata;
mod privacy;
mod proposals;
#[path = "../support/refusal.rs"]
mod refusal;
mod replay;
mod review;
mod seed;
mod switching;

use afd_core::id::Uuid7;
use afd_core::money::NANOS_PER_USD;
use afd_fleet_runtime::Definition;
use serde_json::json;

/// What a funded fixture wallet holds: a thousand dollars, past any reservation
/// a fixture turn or activation can make.
const FUNDED_NANOS: i64 = 1_000 * NANOS_PER_USD;

/// SQLSTATE `lock_not_available`: the pool's lock limit
/// (`afd_db::pool::LOCK_TIMEOUT`) refused a wait, which a caller may retry.
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// How many times a proof retries a lock-limited request before failing.
///
/// A shared lane runs fault fixtures whose trigger DDL queues an exclusive
/// table lock; a request behind it is refused retryably, and its retry
/// converges because every authoring write is keyed by its operation.
const LOCK_RETRIES: usize = 20;

/// Whether `error` is the retryable lock-limit refusal and nothing else.
fn lock_limited(error: &afd_authoring::Error) -> bool {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = cause {
        if let Some(sqlx::Error::Database(database)) = current.downcast_ref::<sqlx::Error>() {
            return database.code().as_deref() == Some(LOCK_NOT_AVAILABLE);
        }
        cause = current.source();
    }
    false
}

/// Runs `attempt` again while the lock limit refuses it, as a client retrying
/// the same keyed request would; any other answer is returned as it is.
async fn retried<T, F, Fut>(mut attempt: F) -> afd_authoring::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = afd_authoring::Result<T>>,
{
    for _ in 1..LOCK_RETRIES {
        match attempt().await {
            Err(error) if lock_limited(&error) => {}
            answered => return answered,
        }
    }
    attempt().await
}

fn mint() -> Uuid7 {
    Uuid7::parse(&afd_db::test_util::mint_id()).expect("fixture UUIDv7")
}

fn definition() -> Definition {
    serde_json::from_value(json!({
        "schema_version": 1,
        "description": "Explain failures.",
        "instructions_md": "Investigate failures.\n",
        "config": {"name": "checkout-watch", "x-agentsfleet": {
            "triggers": [],
            "tools": [], "credentials": [], "network": {"allow": []},
            "gates": {"timeout_ms": 60000, "rules": []},
            "budget": {"daily_nanos": NANOS_PER_USD.to_string()}
        }}
    }))
    .expect("native setup passes the shared validator")
}
