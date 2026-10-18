//! Renewal and dispatch observe expiry after actual database lock waits.

mod authorization;
mod expiry;
mod invalid_usage;
mod lock_wait;
mod suppressed_write;

use super::super::{attempt::Attempt, fixture::Fixture};
use afd_wire::authoring::{Knowledge, Meter, Renew};
use jiff::Timestamp;

/// Expires the attempt's lease at database time, the clock the guard reads.
const EXPIRE_NOW: &str = "UPDATE authoring.attempts \
SET lease_expires_at=date_trunc('milliseconds',clock_timestamp()) WHERE id=$1::uuid";

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

async fn snapshot(f: &Fixture, a: &Attempt) -> (Timestamp, i64, String) {
    let (expires, runtime, hash): (jiff_sqlx::Timestamp, i64, String) = sqlx::query_as("SELECT lease_expires_at,reported_runtime_ms,model_token_hash FROM authoring.attempts WHERE id=$1::uuid")
        .bind(a.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("renewal snapshot");
    (expires.to_jiff(), runtime, hash)
}
