//! Discovery must find fresh work without scanning stopped accounting history.

use super::{Fixture, claim, fixture::fund, history::seed, mint, now, queue, recovery::unknown};
use afd_setup::{
    Claimed, Drafts,
    types::{Draft, SendMessage},
};
use serde_json::Value;
use sqlx::{Connection as _, PgConnection};

const HISTORY: [i64; 5] = [100, 200, 400, 800, 1_000];
const DISCOVERY_LIMIT: i64 = 64;
/// Flat in history: 18 to 23 from 100 to 1,000 stopped attempts a draft. With
/// no Stop watermark to bound the range, each candidate draft reads its newest
/// attempt's row (4 buffers a draft, where the watermark range read none).
const BUFFER_BOUND: u64 = 24;
const EXPLAIN: &str = const_format::concatcp!(
    "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ",
    Drafts::QUEUED_DRAFTS_QUERY
);

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn should_discover_two_queued_drafts_when_stopped_history_grows() {
    let f = fund(Fixture::isolated().await, 4).await;
    let first = Box::pin(stopped(&f)).await;
    let second = Box::pin(stopped(&f)).await;
    let mut owner = PgConnection::connect(&f.owner_url())
        .await
        .expect("isolated plan owner");
    let mut start = 2;
    for last in HISTORY {
        for (draft, run) in [&first, &second] {
            seed(&mut owner, draft, run, start, last).await;
            f.drafts
                .send(
                    f.access(),
                    &draft.id,
                    SendMessage {
                        save: false,
                        message: "Fresh queued work".into(),
                        operation_id: mint(),
                    },
                    now(),
                )
                .await
                .expect("new work past the stopped history");
        }
        let plan: Value = sqlx::query_scalar(EXPLAIN)
            .bind("editing")
            .bind("queued")
            .bind(["awaiting_accounting", "settled"])
            .bind(DISCOVERY_LIMIT)
            // The fixture's platform runner, which every draft may reach.
            .bind(&f.runner)
            .bind("cancelled")
            .fetch_one(&mut owner)
            .await
            .expect("compiled discovery plan");
        indexed(&plan, last);
        for (draft, _) in [&first, &second] {
            f.stop(&draft.id, last + 1).await.expect("Stop queued work");
        }
        start = last + 1;
    }
    owner.close().await.expect("release plan owner");
    f.cleanup().await;
}

async fn stopped(f: &Fixture) -> (Draft, Claimed) {
    let draft = queue(f, "Original unknown request").await;
    let run = claim(f, &draft).await.expect("initial attempt");
    unknown(f, &run).await;
    f.stop(&draft.id, 1).await.expect("Stop old attempt");
    (draft, run)
}

fn indexed(plan: &Value, history: i64) {
    let item = plan
        .as_array()
        .and_then(|items| items.first())
        .expect("query plan item");
    let root = &item["Plan"];
    assert_eq!(root["Actual Rows"].as_f64(), Some(2.0), "{plan}");
    let mut scans = Vec::new();
    attempt_scans(root, &mut scans);
    assert!(!scans.is_empty(), "missing attempts scan: {plan}");
    for scan in scans {
        assert_eq!(
            scan["Index Name"], "uq_attempts_draft_id_message_sequence",
            "{plan}"
        );
        assert_eq!(scan["Scan Direction"], "Backward", "{plan}");
        // The newest attempt only, however long the stopped history.
        assert!(
            scan["Actual Rows"].as_f64().is_some_and(|rows| rows <= 1.0),
            "{plan}"
        );
    }
    let buffers = root["Shared Hit Blocks"].as_u64().expect("buffer count");
    assert!(
        buffers <= BUFFER_BOUND,
        "history={history}, buffers={buffers}: {plan}"
    );
    eprintln!(
        "discovery: 2 queued drafts, {history} stopped attempts each; buffers={buffers}/{BUFFER_BOUND}; execution_ms={}",
        item["Execution Time"]
    );
}

fn attempt_scans<'a>(node: &'a Value, scans: &mut Vec<&'a Value>) {
    if node["Relation Name"] == "attempts" {
        scans.push(node);
    }
    if let Some(children) = node["Plans"].as_array() {
        for child in children {
            attempt_scans(child, scans);
        }
    }
}
