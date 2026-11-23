//! A retried effect lands once: a message, schedule or cron call names its
//! item, `(run_id, call_number)`; the first request writes the
//! item with its answer, and a retry reads that answer back and acts on
//! nothing. A superseded holder is refused before any item is read, and a run
//! that takes a dead run's turn over reads what that run finished.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_fleet::run::effect::{Claim, Effect};
use afd_fleet::run::{Billed, RunService, Standing};
use serde_json::json;

use crate::report_seed::{Held, held};
use crate::rows::{LAPSE, Row};
use crate::seed::{MODEL, POSTURE, PROVIDER};

/// What a post the channel took answers.
const POSTED: &str = r#"{"delivered":true}"#;

/// The tool a message's item names.
const MESSAGE: &str = "message";

/// An item id, minted as the runner mints one: `thread.items` is keyed by it
/// alone, so tests sharing the lane never share an item.
fn item() -> Uuid7 {
    afd_db::test_util::mint_id()
}

/// The live holder's standing.
async fn standing(held: &Held) -> Standing {
    held.fixtures
        .plane()
        .standing(
            &held.runner,
            held.issued.run_id.clone(),
            held.fence.as_u64(),
        )
        .await
        .expect("the live holder stands")
}

/// One message request for call `call_number`, as the handler drives it: the
/// stored answer, or a post counted in `posts` and recorded on the item.
async fn send(
    plane: &RunService,
    standing: &Standing,
    effect: Effect<'_>,
    posts: &std::sync::atomic::AtomicU32,
) -> String {
    match plane
        .claim_effect(standing, effect)
        .await
        .expect("the item claims")
    {
        Claim::Fresh(claimed) => {
            posts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            claimed
                .record(&json!({"text": "working on it"}), POSTED)
                .await
                .expect("the item records its answer");
            POSTED.to_owned()
        }
        Claim::Done(stored) => stored,
    }
}

/// The message effect of call `call_number`, under `held`'s fence.
fn message<'a>(held: &Held, item_id: &'a Uuid7, call_number: u64) -> Effect<'a> {
    Effect {
        item_id,
        call_number,
        fencing_token: held.fence.as_u64(),
        tool: MESSAGE,
    }
}

/// How many items `run` holds.
async fn items_of(held: &Held, run: &Uuid7) -> i64 {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a connection");
    sqlx::query_scalar("SELECT count(*) FROM thread.items WHERE run_id = $1")
        .bind(run)
        .fetch_one(&mut *connection)
        .await
        .expect("the item count")
}

/// The same item sent twice, the second while the first is in flight and
/// once more after it landed, posts once, and every answer is the first's.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retried_message_for_one_item_posts_once() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let standing = standing(&held).await;
    let posts = std::sync::atomic::AtomicU32::new(0);
    let one = item();

    let (first, racing) = tokio::join!(
        send(&plane, &standing, message(&held, &one, 1), &posts),
        send(&plane, &standing, message(&held, &one, 1), &posts),
    );
    let later = send(&plane, &standing, message(&held, &one, 1), &posts).await;

    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!((first.as_str(), racing.as_str()), (POSTED, POSTED));
    assert_eq!(later, POSTED);
    assert_eq!(items_of(&held, &held.issued.run_id).await, 1);
    held.fixtures.cleanup().await;
}

/// Two calls of one run are two items, and each posts.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_calls_in_one_run_post_two_messages() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let standing = standing(&held).await;
    let posts = std::sync::atomic::AtomicU32::new(0);
    let (one, two) = (item(), item());

    send(&plane, &standing, message(&held, &one, 1), &posts).await;
    send(&plane, &standing, message(&held, &two, 2), &posts).await;

    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(items_of(&held, &held.issued.run_id).await, 2);
    held.fixtures.cleanup().await;
}

/// A send at the first fence after a second was assigned is refused before
/// any item is read, so nothing posts. The reassignment expired the first
/// run (`expire_prior_run.sql`), so it is no live run of its runner's:
/// `UZ-RUN-006`.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_effect_from_a_superseded_fence_is_refused() {
    let held = held().await;
    held.fixtures
        .age(Row::Thread(&held.fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("a lapsed assignment is winnable");
    assert!(reassigned.fence > held.fence, "the second fence outranks");

    let refused = held
        .fixtures
        .plane()
        .standing(
            &held.runner,
            held.issued.run_id.clone(),
            held.fence.as_u64(),
        )
        .await
        .err()
        .map(|error| error.code());

    assert_eq!(refused, Some(af_core::error_code::RUN_NOT_FOUND));
    assert_eq!(items_of(&held, &held.issued.run_id).await, 0, "no post");
    held.fixtures.cleanup().await;
}

/// Run 1 posts and dies; run 2, issued for the same turn, carries that post
/// among the calls its recovery note lists.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_resumed_run_note_lists_the_dead_runs_posts() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let standing = standing(&held).await;
    let posts = std::sync::atomic::AtomicU32::new(0);
    let one = item();
    send(&plane, &standing, message(&held, &one, 1), &posts).await;

    held.fixtures
        .age(Row::Thread(&held.fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("a lapsed assignment is winnable");
    let second = held
        .runs
        .issue(
            &held.spare,
            &reassigned,
            Billed {
                tenant_id: &held.tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            held.fixtures.enrolled_at(),
        )
        .await
        .expect("the reassigned run row must be written")
        .expect("the assignment is still held");

    let finished = held
        .runs
        .finished_calls(&held.fleet, &held.turn_id, &second.run_id)
        .await
        .expect("the dead run's calls read");

    let [post] = finished.as_slice() else {
        panic!("one finished call, not {finished:?}");
    };
    assert_eq!(post.tool, MESSAGE);
    assert_eq!(post.output, POSTED);
    assert_eq!(
        post.arguments
            .get("text")
            .and_then(serde_json::Value::as_str),
        Some("working on it")
    );
    assert_eq!(post.call_id, format!("{}:1", held.fence.as_u64()));
    held.fixtures.cleanup().await;
}

/// Run 1 posts and dies; run 2 takes the turn over and records its own calls,
/// then dies before it reports. A third run still lists run 1's post: only
/// settlement drops a dead run's items, so no takeover forgets an effect.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_takeover_still_lists_the_first_dead_runs_posts() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let standing = standing(&held).await;
    let posts = std::sync::atomic::AtomicU32::new(0);
    let one = item();
    send(&plane, &standing, message(&held, &one, 1), &posts).await;

    held.fixtures
        .lapse_holder(&held.issued.run_id, &held.fleet)
        .await;
    let reassigned = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("a lapsed assignment is winnable");
    let second = held
        .runs
        .issue(
            &held.spare,
            &reassigned,
            Billed {
                tenant_id: &held.tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            held.fixtures.enrolled_at(),
        )
        .await
        .expect("the reassigned run row must be written")
        .expect("the assignment is still held");
    let records = format!(
        r#"{{"fencing_token":{},"calls":[{{"call_number":1,"arguments":{{}},"truncated_arguments":false,"output":"ok","output_line_count":1,"truncated":false,"turn_id":"{}","kind":"tool_call","tool":"shell","status":"succeeded"}}]}}"#,
        reassigned.fence.as_u64(),
        held.turn_id.as_str()
    );
    let records: af_wire::tool_detail::ToolCallRecordsRequest<'_> =
        serde_json::from_str(&records).expect("a well-formed post");
    plane
        .record_tool_calls(&held.spare, second.run_id.as_str(), &records, held.now)
        .await
        .expect("run 2 records its calls");

    // The third run's own id only excludes its items; it has none yet.
    let third = item();
    let finished = held
        .runs
        .finished_calls(&held.fleet, &held.turn_id, &third)
        .await
        .expect("the dead runs' calls read");

    let tools: Vec<&str> = finished.iter().map(|call| call.tool.as_ref()).collect();
    assert_eq!(tools, [MESSAGE, "shell"], "run 1's post survives run 2's");
    held.fixtures.cleanup().await;
}
