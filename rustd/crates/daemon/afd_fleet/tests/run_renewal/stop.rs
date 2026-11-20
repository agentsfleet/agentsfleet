//! Stop on a draft's running turn ends its setup run, which reports stopped.
//!
//! The draft door of the one Stop: the creator's PATCH reaches
//! `Drafts::stop_turn`, which marks the setup run and sends its runner the
//! Cancel; the runner's `stopped` report then settles the run, and the turn
//! stays cancelled with no answer.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use super::{fixture::Running, replica::expire, seat::Seat};
use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use af_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};
use af_wire::run::{AcceptedKinds, SetupReport, TaggedRun};
use afd_bus::{Command, Received, Subject};
use afd_fleet::run::OfferRequest;
use afd_turns::view::stop::Stop;

const KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const WAIT: Duration = Duration::from_secs(5);

/// The draft run the host was given: its id, turn and fence.
struct Taken {
    run: String,
    turn: String,
    fence: u64,
}

// Value: protects=Stop ends a setup draft's running turn as it ends a fleet's;
// fails_when=the draft door skips the run's mark or hint, or the report answers the turn;
// why_new=the draft Stop was a sequence watermark, with no per-turn door;
// seam=the production draft claim, Drafts::stop_turn over a real bus, and report_draft.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn stopping_a_running_draft_turn_ends_its_setup_run() {
    let running = Running::create_isolated().await;
    let seat = Seat::create(&running).await;
    let (bus, _server) = afd_bus::test_util::detached().expect("an in-process bus opens");
    let drafts = Seat::drafts(&running.fixtures.database).with_bus(bus.clone());
    let draft = seat.queue(&drafts).await;
    expire(&running).await;
    let taken = take_draft_run(&running, &drafts).await;
    let address = Subject::RunnerCommand(running.runner.clone());
    let mut commands = bus.subscribe(&address).await.expect("commands subscribe");
    let turn = Uuid7::parse(&taken.turn).expect("the turn is a UUIDv7");

    let stopped = drafts
        .stop_turn(seat.access(), &draft.id, &turn)
        .await
        .expect("Stop answers")
        .expect("the draft holds the turn");
    assert!(
        matches!(stopped.stop, Stop::Cancelled { running: true, .. }),
        "a running draft turn is cancelled: {stopped:?}"
    );
    let run = Uuid7::parse(&taken.run).expect("the run is a UUIDv7");
    assert_cancel(&mut commands, &run).await;

    let creations = afd_setup::Creations::new(
        afd_setup::Setup::new(
            drafts.clone(),
            Arc::new(Kek::from_hex(KEK_HEX).expect("fixture key")),
        ),
        afd_turns::Turns::new(running.fixtures.database.clone(), bus, Entropy::new()),
    );
    running
        .fixtures
        .plane()
        .report_draft(
            &creations,
            &running.runner,
            &stopped_report(&taken),
            af_core::clock::now(),
        )
        .await
        .expect("a stopped draft run reports");
    let (run_status, turn_status, answer): (String, String, Option<String>) = sqlx::query_as(
        "SELECT r.status, t.status, t.answer_text FROM thread.runs r \
         JOIN thread.turns t ON t.id = r.turn_id WHERE r.id = $1",
    )
    .bind(&run)
    .fetch_one(&mut *running.fixtures.database.acquire().await.expect("read"))
    .await
    .expect("the run and its turn");
    assert_eq!(run_status, afd_state::sql::RUN_STATUS_STOPPED);
    assert_eq!(turn_status, af_core::turn::status::CANCELLED);
    assert_eq!(answer, None, "a stopped turn gets no answer");
    drop(commands);
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}

/// The host's next run, which must be the seat's queued draft turn.
async fn take_draft_run(running: &Running, drafts: &afd_setup::Drafts) -> Taken {
    running
        .fixtures
        .plane()
        .claim_with(
            drafts,
            OfferRequest {
                runner: &running.runner,
                held: &[],
                degraded: false,
                now: running.now,
                recovery: None,
            },
            AcceptedKinds::ALL,
            true,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |run, _metadata| match run {
                TaggedRun::Fleet(run) if run.setup.is_some() => Some(Taken {
                    run: run.run_id.to_string(),
                    turn: run.event.turn_id.to_string(),
                    fence: run.fencing_token,
                }),
                TaggedRun::Fleet(_) | TaggedRun::Authoring(_) => None,
            },
        )
        .await
        .expect("the poll answers")
        .flatten()
        .expect("the queued draft turn is placed as a draft run")
}

/// Waits for the Cancel naming `run` on the runner's command subject.
async fn assert_cancel(commands: &mut afd_bus::Subscription, run: &Uuid7) {
    tokio::time::timeout(WAIT, async {
        loop {
            if let Ok(Received::Message(message)) = commands.recv().await
                && let Ok(Command::Cancel { run_id }) = serde_json::from_str(&message.payload)
            {
                assert_eq!(&run_id, run);
                return;
            }
        }
    })
    .await
    .expect("the Cancel reaches the runner's subject");
}

/// The report a runner sends for a draft run the daemon stopped.
fn stopped_report(taken: &Taken) -> ReportRequest<'_> {
    ReportRequest {
        folded_through_sequence: None,
        setup: Some(SetupReport { proposal: None }),
        task: None,
        run_id: Cow::Borrowed(&taken.run),
        turn_id: Cow::Borrowed(&taken.turn),
        fencing_token: taken.fence,
        outcome: Outcome::Stopped,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(""),
        tokens: 0,
        requests: af_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: 1,
            segments: None,
        },
        checkpoint: ReportCheckpoint {
            last_turn_id: Cow::Borrowed(&taken.turn),
            last_response: Cow::Borrowed(""),
        },
        tool_calls: None,
        held_until: None,
        memory: Vec::new(),
    }
}
