//! Discovery must find fresh work without scanning stopped accounting history.
#![expect(
    clippy::print_stderr,
    reason = "the test prints its measured evidence for the lane log"
)]

use super::{Fixture, claim, fixture::fund, history::seed, mint, now, queue, recovery::unknown};
use afd_authoring::{
    Claimed, Drafts,
    types::{Draft, SendMessage},
};
use serde_json::Value;
use sqlx::{Connection as _, PgConnection};

const HISTORY: [i64; 5] = [100, 200, 400, 800, 1_000];
const DISCOVERY_LIMIT: i64 = 64;
const BUFFER_BOUND: u64 = 16;
const EXPLAIN: &str = const_format::concatcp!(
    "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ",
    Drafts::QUEUED_DRAFTS_QUERY
);

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn should_discover_two_queued_drafts_when_stopped_history_grows() {
    let f = fund(Fixture::isolated().await, 4).await;
    let first = stopped(&f).await;
    let second = stopped(&f).await;
    let mut owner = PgConnection::connect(&f.owner_url())
        .await
        .expect("isolated plan owner");
    let mut start = 2;
    for last in HISTORY {
        for (draft, lease) in [&first, &second] {
            seed(&mut owner, draft, lease, start, last).await;
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
                .expect("new work past cutoff");
        }
        let plan: Value = sqlx::query_scalar(EXPLAIN)
            .bind("editing")
            .bind("queued")
            .bind("settled")
            .bind(DISCOVERY_LIMIT)
            .fetch_one(&mut owner)
            .await
            .expect("compiled discovery plan");
        indexed(&plan, last);
        for (draft, _) in [&first, &second] {
            f.drafts
                .cancel(f.access(), &draft.id, last + 1)
                .await
                .expect("Stop queued work");
        }
        start = last + 1;
    }
    owner.close().await.expect("release plan owner");
    f.cleanup().await;
}

async fn stopped(f: &Fixture) -> (Draft, Claimed) {
    let draft = queue(f, "Original unknown request").await;
    let lease = claim(f, &draft).await.expect("initial attempt");
    unknown(f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("Stop old attempt");
    (draft, lease)
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
        assert!(
            scan["Index Cond"]
                .as_str()
                .expect("attempt index condition")
                .contains("message_sequence >"),
            "{plan}"
        );
        assert_eq!(scan["Actual Rows"].as_f64(), Some(0.0), "{plan}");
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
