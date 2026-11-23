//! A run too large for the runner protocol's frame fails at assignment.
//!
//! No route accepts a body that fills the 32 MiB frame: a fleet's message to
//! another, at the text ceiling in three-byte characters, is a 3 MiB body the
//! steer route takes and the target's run carries. So the test stores that
//! steer, then grows its stored body past the frame, standing in for any
//! field that outgrows it. The daemon fails that turn the moment it would
//! have sent it, under its own code, instead of handing a runner a frame the
//! encoder refuses and letting the run lapse and be reassigned.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use af_core::id::Uuid7;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;

use crate::assignments::steered;
use crate::steers::Crew;
use crate::wire::{report, report_body};

/// `af_validate::TEXT_MAX_CHARS`, mirrored: a suite that imported it could
/// not tell a bound that moved from one that was always this.
const CEILING_CHARS: usize = 1_048_576;

/// Three UTF-8 bytes, one character.
const THREE_BYTES: char = '界';

/// Characters of [`THREE_BYTES`] whose UTF-8 alone passes the runner frame
/// (`af_wire::protocol::MESSAGE_BYTES_MAX`, 32 MiB).
const PAST_THE_FRAME_CHARS: i32 = 32 * 1024 * 1024 / 3 + 1;

/// How long the turn is given to fail once its sender's run has settled.
const FAIL_WITHIN: Duration = Duration::from_secs(10);

/// A turn's status, failure class and failure sentence, as the run left it.
type Failure = (String, Option<String>, Option<String>);

/// `turn`'s status and failure, once it is no longer waiting.
async fn ended(crew: &Crew, turn: &Uuid7) -> Option<Failure> {
    let mut connection = crew
        .run()
        .booted
        .database
        .acquire()
        .await
        .expect("a connection");
    let row: Failure = sqlx::query_as(
        "SELECT status, failure_reason, failure_detail FROM thread.turns WHERE id = $1",
    )
    .bind(turn)
    .fetch_one(&mut *connection)
    .await
    .expect("the turn reads");
    (row.0 == af_core::turn::status::FLEET_ERROR).then_some(row)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_run_too_large_for_its_frame_fails_with_its_code() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let target = crew.running_sibling("frame-target").await;
    let (run, fence) = crew.speaking.run_seeded().await;
    let words: String = std::iter::repeat_n(THREE_BYTES, CEILING_CHARS).collect();

    let (status, answer) = crew.steer((&run, fence), &target, &words).await;
    assert_eq!(status, 202, "{answer}");
    let turn = steered(&answer);
    let stored = crew
        .turns(&target)
        .await
        .into_iter()
        .find(|row| row.id == turn)
        .expect("the steer is stored on the target");
    let body: Value = serde_json::from_str(&stored.request_json).expect("a stored body");
    assert_eq!(body.get("text"), None, "the words are stored once");
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .expect("the message");
    assert!(message.ends_with(&words), "the message carries the words");
    assert_eq!(
        stored.request_json.matches(THREE_BYTES).count(),
        CEILING_CHARS
    );
    // Only the words grow, so the body keeps the steer's shape; built in
    // Postgres, so the test never holds a second 32 MiB copy.
    sqlx::query(
        "UPDATE thread.turns \
         SET request_json = jsonb_set(request_json, ARRAY['message'], to_jsonb(repeat($2, $3))) \
         WHERE id = $1",
    )
    .bind(&turn)
    .bind(THREE_BYTES.to_string())
    .bind(PAST_THE_FRAME_CHARS)
    .execute(
        &mut *crew
            .run()
            .booted
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the stored body grows past the frame");

    // The sender's run settles, freeing the one worker for the target's turn.
    let settled = report(crew.run(), &report_body(&run, &crew.run().turn_id, fence)).await;
    assert!(settled.is_ok(), "{settled:?}");
    let failing = async {
        loop {
            if let Some(failure) = ended(&crew, &turn).await {
                return failure;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    let (_, class, detail) = tokio::time::timeout(FAIL_WITHIN, failing)
        .await
        .expect("the turn fails at assignment, without waiting out a lapse");
    assert_eq!(class.as_deref(), Some("frame_too_large"));
    let detail = detail.expect("a failure sentence");
    assert!(detail.starts_with("[UZ-RUN-032] "), "{detail}");

    let runs: Vec<i64> =
        sqlx::query_scalar("SELECT fencing_token FROM thread.runs WHERE turn_id = $1")
            .bind(&turn)
            .fetch_all(
                &mut *crew
                    .run()
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("a connection"),
            )
            .await
            .expect("the turn's runs read");
    assert_eq!(runs.len(), 1, "one run, never reassigned: {runs:?}");
    crew.finish(supervisor).await;
}
