//! A person's first message is the fleet: the create answers with the new
//! fleet's first turn, a runner on the other daemon answers it, and the
//! completion reaches the fleet's stream on the daemon the person called.

use super::{TestResult, fixture::Fixture, signals};
use crate::wire;
use af_core::id::Uuid7;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use serde_json::json;
use std::time::Duration;

/// What the person says first.
const FIRST_MESSAGE: &str = "Triage the failing deploy";

/// How long the stream is held open; the run settles in well under it.
const STREAM_WINDOW: Duration = Duration::from_secs(30);

/// How many assignments the runner takes looking for the new fleet's turn:
/// the scenario's own seeded turn may come first, and a fleet another suite
/// created with no placement tag may still hold one.
const ASSIGNMENTS: usize = 8;

/// What the raw runner's report answers (`wire::report_body`).
const ANSWER: &str = "the fixture scene produced this";

/// The frame a settled turn publishes on its fleet's stream.
const COMPLETE: &str = r#""kind":"turn_complete""#;

// Value: protects=a first message makes a fleet whose first turn runs and whose
// completion reaches the person's stream; fails_when=the create writes no turn,
// the turn is not offered to a runner, or the completion does not cross the bus;
// why_new=the draft path this replaces had its own suite; seam=two production
// daemons, the real bus, real Postgres, a raw native runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_first_message_is_answered_on_the_fleet_stream() -> TestResult {
    let mut fixture = Fixture::start().await?;
    let created = fixture
        .operator
        .create_fleet(&fixture.scene, FIRST_MESSAGE)
        .await?;
    let id = |pointer: &str| {
        wire::pointer(&created, pointer)
            .as_str()
            .ok_or("an identifier in the create")
            .map(Uuid7::parse)
    };
    let fleet = id("/fleet/id")??;
    let turn = id("/accepted/turn_id")??;
    fixture.adopt(fleet.clone());
    let mut stream = fixture
        .operator
        .stream(&fixture.scene, &fleet, STREAM_WINDOW)
        .await?;

    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native
        .send(
            StreamKind::Control,
            &json!({"type":"ready","credits":2,"revision":1,"observed_assignments":0}),
        )
        .await;
    let mut answered = false;
    for _ in 0..ASSIGNMENTS {
        let held = signals::assign(&mut native).await?;
        signals::report(&fixture.scene, &mut native, &held, false).await?;
        if held.event == turn {
            assert_eq!(held.fleet, fleet, "the first turn runs as the new fleet's");
            answered = true;
            break;
        }
    }
    assert!(answered, "the new fleet's first turn was never assigned");

    let frame = completion(&mut stream, &turn).await?;
    assert!(
        frame.contains(ANSWER),
        "the runner's answer rides the frame: {frame}"
    );
    native.close().await;
    fixture.finish().await
}

/// Reads `stream` until the completion frame of `turn` arrives, answering it.
async fn completion(stream: &mut reqwest::Response, turn: &Uuid7) -> TestResult<String> {
    let mut text = String::new();
    let read = async {
        while let Some(chunk) = stream.chunk().await? {
            text.push_str(&String::from_utf8_lossy(&chunk));
            let found = text
                .split("\n\n")
                .find(|frame| frame.contains(COMPLETE) && frame.contains(turn.as_str()));
            if let Some(frame) = found {
                return Ok::<_, reqwest::Error>(Some(frame.to_owned()));
            }
        }
        Ok(None)
    };
    let frame = tokio::time::timeout(STREAM_WINDOW, read).await??;
    Ok(frame.ok_or("the stream closed before the turn completed")?)
}
