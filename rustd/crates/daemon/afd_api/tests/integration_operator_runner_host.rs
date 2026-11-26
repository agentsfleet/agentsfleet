//! The host facts a runner's beat carries, stored on its row and served by the
//! operator's runner view.

use af_core::test_util::trace::Capture;
use af_crypto::entropy::Entropy;
use af_wire::runner::{HeartbeatRequest, HostFacts};
use afd_auth::scope::{Scope, ScopeSet};
use afd_db::test_util::mint_id;
use afd_runner::Runners;
use http::{Method, StatusCode};

use super::{Fixture, NOW, SUBJECT};
use crate::harness::{Fleet, json_body, send};

/// Facts a host could report, inside every bound.
const FACTS: HostFacts = HostFacts {
    cpu_count: Some(8),
    memory_bytes: Some(17_179_869_184),
    free_disk_bytes: Some(34_359_738_368),
};

/// The stored facts and the last beat, as the runner row holds them.
const STORED: &str = "SELECT host_cpu_count, host_memory_bytes, host_free_disk_bytes, \
    (EXTRACT(EPOCH FROM last_seen_at) * 1000)::bigint FROM runner.runners WHERE id = $1";

/// Each fact column, then the last beat in milliseconds since the epoch.
type Stored = (Option<i32>, Option<i64>, Option<i64>, i64);

async fn beat(runners: &Runners, fixture: &Fixture, host: HostFacts) {
    let beat = HeartbeatRequest {
        host,
        ..afd_runner::NO_REPORT
    };
    runners
        .heartbeat(&fixture.runner, &beat, None, NOW)
        .await
        .expect("the beat lands");
}

async fn stored(fixture: &Fixture) -> Stored {
    sqlx::query_as(STORED)
        .bind(&fixture.runner)
        .fetch_one(&mut *fixture.database.acquire().await.expect("an API connection"))
        .await
        .expect("the runner row reads")
}

// Value: protects=the host facts a beat carries reach the runner row and the
// operator's runner view; fails_when=the beat's bump stops storing them or the
// view stops reading them; seam=the heartbeat statement and the operator router
// over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_beats_host_facts_reach_the_operator_runner_view() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    beat(
        &Runners::new(fixture.database.clone(), Entropy::new()),
        &fixture,
        FACTS,
    )
    .await;
    let router = Fleet::live(
        fixture.database.clone(),
        SUBJECT,
        ScopeSet::from_scopes(&Scope::ALL),
    )
    .router();
    let path = format!("/v1/admin/runners/{}", fixture.runner);

    let detail = send(&router, Method::GET, &path, Some(&fixture.token), "").await;

    assert_eq!(detail.status(), StatusCode::OK);
    let detail = json_body(detail).await;
    assert_eq!(
        detail.get("host"),
        Some(&serde_json::json!({
            "cpu_count": 8,
            "memory_bytes": 17_179_869_184_u64,
            "free_disk_bytes": 34_359_738_368_u64,
        })),
        "{detail}"
    );
    fixture.cleanup(&mint_id()).await;
}

// Value: protects=facts past their wire bounds never reach the row, which keeps
// the last facts while the beat's liveness still lands, and the refusal is
// logged; fails_when=the beat stores facts its bounds refuse, or drops them
// silently; seam=the heartbeat's proof and statement over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn facts_past_their_bounds_keep_the_last_facts_and_the_beat_lands() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let runners = Runners::new(fixture.database.clone(), Entropy::new());
    beat(&runners, &fixture, FACTS).await;
    let first = stored(&fixture).await;
    let refused = HostFacts {
        cpu_count: Some(0),
        memory_bytes: Some(u64::MAX),
        free_disk_bytes: Some(1),
    };

    let log = Capture::install();
    beat(&runners, &fixture, refused).await;
    let line = log.only("runner_host_facts_refused");
    drop(log);

    let second = stored(&fixture).await;
    assert_eq!(
        (second.0, second.1, second.2),
        (Some(8), Some(17_179_869_184), Some(34_359_738_368)),
        "the row keeps the last facts"
    );
    assert!(second.3 > first.3, "the beat's liveness lands");
    assert_eq!(
        (line.field("error_code"), line.field("fields")),
        (
            Some(af_core::error_code::INVALID_REQUEST.as_str()),
            Some("cpu_count,memory_bytes")
        )
    );
    fixture.cleanup(&mint_id()).await;
}
