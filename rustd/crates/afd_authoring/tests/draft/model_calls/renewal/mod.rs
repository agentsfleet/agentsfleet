//! Renewal and dispatch observe expiry after actual database lock waits.

mod authorization;
mod expiry;
mod invalid_usage;
mod lock_wait;
mod suppressed_write;

use super::super::{attempt::Attempt, fixture::Fixture};
use afd_db::statement::NOW_MS;
use afd_wire::authoring::{Knowledge, Meter, Renew};

/// Expires the attempt's lease at database time, the clock the guard reads.
const EXPIRE_NOW: &str = const_format::concatcp!(
    "UPDATE fleet.authoring_attempts SET lease_expires_at=",
    NOW_MS,
    " WHERE id=$1::uuid"
);

fn request() -> Renew {
    Renew {
        fence: "1".into(),
        deletion_generation: "0".into(),
        usage: Meter {
            knowledge: Knowledge::Known,
            calls: 0,
            input_tokens: Some(0),
            output_tokens: Some(0),
            runtime_ms: 0,
            provider_cost_nanos: Some("0".into()),
        },
    }
}

async fn snapshot(f: &Fixture, a: &Attempt) -> (i64, i64, String) {
    sqlx::query_as("SELECT lease_expires_at,reported_runtime_ms,capability_hash FROM fleet.authoring_attempts WHERE id=$1::uuid")
        .bind(a.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("renewal snapshot")
}
