//! Retained progress makes a timed-out fairness check explain what stopped.

use super::{Hot, Scenario, wire};
use af_core::{id::Uuid7, timing::OFFER_FALLBACK_TICK};
use af_wire::{protocol::Down, run::TaggedRun};
use afd_db::Db;

const PROBES: &str = "SELECT calls FROM pg_stat_statements \
    WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database()) \
      AND query LIKE $1";
const PROBE_PATTERN: &str = "%FROM thread.turns a%FROM setup.drafts d%";

#[derive(Debug, Default)]
pub(super) struct Witness {
    offers: usize,
    old: bool,
    hot: bool,
}

pub(super) struct Offered {
    pub(super) fleet: Uuid7,
    event: Uuid7,
    run: String,
    fence: u64,
}

pub(super) async fn offered(scene: &Scenario) -> Offered {
    let frame = tokio::time::timeout(
        OFFER_FALLBACK_TICK + af_core::timing::STREAM_ANSWER_TIMEOUT,
        scene.native_session().await.offer(),
    )
    .await
    .expect("a ready worker receives a bounded native offer")
    .expect("native offer");
    let decoded = frame
        .decode::<Down<'_>>()
        .expect("the current offer schema");
    let run = match decoded {
        Down::Run {
            run: TaggedRun::Fleet(run),
        } => Some(run),
        _ => None,
    }
    .expect("only fleet work was seeded");
    Offered {
        fleet: Uuid7::parse(&run.event.fleet_id).expect("fleet id"),
        event: Uuid7::parse(&run.event.event_id).expect("event id"),
        run: run.run_id.to_string(),
        fence: run.fencing_token,
    }
}

pub(super) async fn progress(
    scene: &Scenario,
    owner: &Db,
    hot: &Hot,
    first: Offered,
    before: u64,
    witness: &mut Witness,
) {
    let mut next = first;

    loop {
        // Ensure a hot hint and another eligible hot admission precede every capacity release.
        hot.admit().await;
        wire::report(scene, &zero_usage(&next))
            .await
            .expect("the native report settles");
        witness.offers += 1;
        witness.old |= next.event == scene.event_id;
        witness.hot |= next.fleet == hot.fleet;
        if witness.old && witness.hot && probes(owner).await >= before + 2 {
            break;
        }
        next = offered(scene).await;
    }
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM thread.turns WHERE id = $1 AND thread_id = $2",
    )
    .bind(&scene.event_id)
    .bind(&scene.fleet)
    .fetch_one(&mut *owner.acquire().await.expect("the result connection"))
    .await
    .expect("the original admission remains readable");
    assert!(
        settled,
        "older work reached its durable result while hot demand continued"
    );
}

pub(super) fn zero_usage(offer: &Offered) -> serde_json::Value {
    let mut body = wire::report_body(&offer.run, &offer.event, offer.fence);
    let fields = body
        .as_object_mut()
        .expect("the report fixture is an object");
    fields.insert("tokens".into(), serde_json::json!(0));
    fields.insert("requests".into(), serde_json::json!([]));
    body
}

pub(super) async fn probes(database: &Db) -> u64 {
    let calls: Vec<i64> = sqlx::query_scalar(PROBES)
        .bind(PROBE_PATTERN)
        .fetch_all(
            &mut *database
                .acquire()
                .await
                .expect("statement counter connection"),
        )
        .await
        .expect("the real probe counter is readable");
    // A missing baseline is zero; the final assertion requires two actual matching executions.
    calls
        .into_iter()
        .map(|count| u64::try_from(count).expect("statement counts are nonnegative"))
        .sum()
}

pub(super) async fn wait_for_probes(database: &Db, target: u64) {
    while probes(database).await < target {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
