//! Stop ends a running fleet turn: the PATCH marks the run, the Cancel reaches
//! the runner's connection on another replica, and the run reports stopped.

use super::super::raw::Native;
use super::{TestResult, faults, fixture::Fixture, signals};
use crate::wire;
use af_core::id::Uuid7;
use af_stream::Transport;
use af_wire::{protocol::StreamKind, turn::TurnKind};
use afd_bus::Subject;
use afd_turns::view::stop::Stop;
use serde_json::json;
use std::sync::atomic::Ordering;

// Value: protects=the Goal: a person stops a running fleet turn and the runner ends the run;
// fails_when=the PATCH skips the mark or the hint, or the report answers the cancelled turn;
// why_new=no route stopped a fleet turn before §8;
// seam=two production daemons, authenticated HTTP, real Bus and Postgres, raw native runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stopping_a_running_fleet_turn_ends_its_run() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    let (status, body) = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &held.event)
        .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "stop answered {body}");
    assert_eq!(wire::pointer(&body, "/status"), "cancelled");
    assert_eq!(wire::pointer(&body, "/turn_id"), held.event.as_str());
    // The Cancel reaches the other replica's connection within the push bound,
    // which is far inside one renewal interval.
    signals::cancelled(&mut native, &held).await?;
    report_stopped(&mut native, &held).await?;
    assert_stopped(&fixture, &held, &[&held.event]).await?;
    native.close().await;
    fixture.finish().await
}

// Value: protects=a Stop whose Cancel is lost still ends the run;
// fails_when=Stop authority depends on the hint rather than the durable mark;
// why_new=the archive's lost-cancel test does not prove the Stop door marks the run;
// seam=the production Stop statement and announce, with only the runner command dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stop_whose_hint_was_lost_ends_at_the_next_renewal() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    let (bus, dropped) = faults::dropping(
        fixture.bus.clone(),
        Subject::RunnerCommand(fixture.scene.runner_id.clone()),
    );
    // The two calls the fleet Stop handler makes, over a bus that loses the hint.
    let history = afd_turns::view::History::new(fixture.scene.booted.database.clone());
    let stopped = history
        .stop(&fixture.scene.workspace, &fixture.scene.fleet, &held.event)
        .await?
        .ok_or("the running turn is found")?;
    let Stop::Cancelled { runs, running, .. } = stopped.stop else {
        return Err("a running turn is cancelled".into());
    };
    assert!(running, "a live run served the turn");
    afd_turns::view::stop::announce(&bus, runs).await;
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "the Cancel hint was lost"
    );
    let answers = signals::renew(&mut native, &[&held], 4).await;
    let refused = wire::pointer(&answers, "/runs/0/answer/error_code");
    assert_eq!(refused, af_core::error_code::RUN_CANCELLED.as_str());
    report_stopped(&mut native, &held).await?;
    assert_stopped(&fixture, &held, &[&held.event]).await?;
    native.close().await;
    fixture.finish().await
}

// Value: protects=Stop cancels every turn one run serves, and the run reports once;
// fails_when=Stop finds the run only through its head, or leaves a folded turn to be answered;
// why_new=no test stopped a turn a run folded in;
// seam=real intake, the native fold assignment, the HTTP PATCH and the report.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stopping_one_of_two_served_turns_cancels_both() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native.ready().await;
    let held = signals::assign(&mut native).await?;
    let second = admit_person_message(&fixture).await?;
    fold(&mut native, &held, &second).await;
    let (status, body) = fixture
        .operator
        .stop_turn(&fixture.scene, &fixture.scene.workspace, &second)
        .await?;
    assert_eq!(status, reqwest::StatusCode::OK, "stop answered {body}");
    signals::cancelled(&mut native, &held).await?;
    report_stopped(&mut native, &held).await?;
    assert_stopped(&fixture, &held, &[&held.event, &second]).await?;
    let reports: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.runs WHERE fleet_id=$1 AND turn_id=$2 AND status<>'active'",
    )
    .bind(&fixture.scene.fleet)
    .bind(&held.event)
    .fetch_one(&mut *fixture.scene.booted.database.acquire().await?)
    .await?;
    assert_eq!(reports, 1, "one run report settles both turns");
    native.close().await;
    fixture.finish().await
}

/// Reports `held` as a runner does once the daemon stopped it: `stopped`, no
/// answer, its usage still billed.
pub(super) async fn report_stopped(native: &mut Native, held: &signals::Held) -> TestResult {
    let mut report = wire::report_body(&held.run, &held.event, held.fence);
    let fields = report.as_object_mut().ok_or("report object missing")?;
    fields.insert("outcome".into(), "stopped".into());
    fields.insert("response_text".into(), "".into());
    native
        .send(
            StreamKind::Reports,
            &json!({"type":"report","request_id":20,"request":report}),
        )
        .await;
    let ack = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!(wire::pointer(&ack, "/answer/status"), "ok");
    Ok(())
}

/// The run settled `stopped` with its usage billed, and every turn in
/// `turns` is cancelled, settled, and unanswered.
pub(super) async fn assert_stopped(
    fixture: &Fixture,
    held: &signals::Held,
    turns: &[&Uuid7],
) -> TestResult {
    let mut connection = fixture.scene.booted.database.acquire().await?;
    let run = Uuid7::parse(&held.run)?;
    let (status, marked): (String, bool) = sqlx::query_as(
        "SELECT status, cancel_requested_at IS NOT NULL FROM thread.runs WHERE id=$1",
    )
    .bind(&run)
    .fetch_one(&mut *connection)
    .await?;
    assert_eq!(status, afd_state::sql::RUN_STATUS_STOPPED);
    assert!(marked, "Stop committed the durable cancel mark");
    let billed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing.usage_lines WHERE work_id=$1")
            .bind(&run)
            .fetch_one(&mut *connection)
            .await?;
    assert!(billed > 0, "a stopped run's usage is billed");
    for turn in turns {
        let (status, answer, settled): (String, Option<String>, bool) = sqlx::query_as(
            "SELECT status, answer_text, settled_at IS NOT NULL FROM thread.turns WHERE id=$1",
        )
        .bind(turn)
        .fetch_one(&mut *connection)
        .await?;
        assert_eq!(status, af_core::turn::status::CANCELLED, "turn {turn}");
        assert_eq!(answer, None, "a stopped turn gets no answer");
        assert!(
            settled,
            "a cancelled turn is settled, so no assignment takes it"
        );
    }
    Ok(())
}

/// A person's later message in the running turn's conversation, which the
/// run may fold in.
async fn admit_person_message(fixture: &Fixture) -> TestResult<Uuid7> {
    let actor = afd_turns::view::steer_actor("user_2stopfixture");
    let admitted =
        afd_turns::Turns::for_tests(fixture.scene.booted.database.clone(), fixture.bus.clone())
            .admit(afd_turns::IncomingTurn {
                producer: afd_turns::Producer::Steer,
                key: afd_turns::Key::Unrepeatable,
                fleet: &fixture.scene.fleet,
                workspace: &fixture.scene.workspace,
                actor: &actor,
                event_type: TurnKind::Chat,
                request_json: r#"{"message":"also check staging"}"#,
                reply: afd_turns::Reply::None,
            })
            .await?;
    Ok(admitted.stored.id)
}

/// The run's fold assignment, through its native lane: it takes `second`.
async fn fold(native: &mut Native, held: &signals::Held, second: &Uuid7) {
    native
        .send(
            StreamKind::Work,
            &json!({
                "type":"folds", "request_id":7, "run_id":held.run,
                "request":{"fencing_token":held.fence,"after_sequence":0}
            }),
        )
        .await;
    let folded = native.expect(StreamKind::Work, "folded").await;
    assert_eq!(
        wire::pointer(&folded, "/answer/response/inputs/0/turn_id"),
        second.as_str(),
        "the run folded the second message: {folded}"
    );
}
