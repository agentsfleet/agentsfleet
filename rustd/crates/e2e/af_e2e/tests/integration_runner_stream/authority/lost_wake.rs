//! A lost wake, reproduced: the ring a commit sends after it is dropped while
//! every subscription stays connected, so the turn waits for the next event.

use super::{TestResult, faults, fixture::Fixture, signals};
use af_core::id::Uuid7;
use af_stream::Transport;
use af_wire::{protocol::StreamKind, turn::TurnKind};
use afd_bus::{Handle, Subject};
use serde_json::json;
use std::sync::atomic::Ordering;

// Value: protects=the accepted loss stays visible: one dropped ring leaves its turn queued,
// and the next ring issues it first; fails_when=a timer starts finding queued work, or a
// later ring stops carrying the earlier turn; why_new=no test showed the case; seam=two
// production daemons, the real bus with one ring dropped, real Postgres, raw native runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_lost_wake_leaves_the_turn_queued_until_the_next_event() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut native = fixture.native(Transport::H2).await;
    native.heartbeat().await;
    native
        .send(
            StreamKind::Control,
            &json!({"type":"ready","credits":2,"revision":1,"observed_assignments":0}),
        )
        .await;
    let seeded = signals::assign(&mut native).await?;
    signals::report(&fixture.scene, &mut native, &seeded, false).await?;

    let (dropping, dropped) = faults::dropping(
        fixture.bus.clone(),
        Subject::ThreadWake(fixture.scene.fleet.clone()),
    );
    let lost = admit(&fixture, &dropping, "check the deploy").await?;
    assert_eq!(dropped.load(Ordering::SeqCst), 1, "the ring was lost");
    native.heartbeat().await;
    native.heartbeat().await;
    assert_eq!(
        runs_for(&fixture, &lost).await?,
        0,
        "no run serves the turn whose ring was lost"
    );

    let next = admit(&fixture, &fixture.bus, "and the rollback").await?;
    let held = signals::assign(&mut native).await?;
    assert_eq!(
        held.event, lost,
        "the next ring issues the earlier turn first"
    );
    assert_ne!(held.event, next);
    native.close().await;
    fixture.finish().await
}

async fn admit(fixture: &Fixture, bus: &Handle, message: &str) -> TestResult<Uuid7> {
    let actor = afd_turns::view::steer_actor("user_2lostwakefixture");
    let request = json!({ "message": message }).to_string();
    let admitted = afd_turns::Turns::for_tests(fixture.scene.booted.database.clone(), bus.clone())
        .admit(afd_turns::IncomingTurn {
            producer: afd_turns::Producer::Steer,
            key: afd_turns::Key::Unrepeatable,
            fleet: &fixture.scene.fleet,
            workspace: &fixture.scene.workspace,
            actor: &actor,
            event_type: TurnKind::Chat,
            request_json: &request,
            reply: afd_turns::Reply::None,
        })
        .await?;
    Ok(admitted.stored.id)
}

async fn runs_for(fixture: &Fixture, turn: &Uuid7) -> TestResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE turn_id=$1")
            .bind(turn)
            .fetch_one(&mut *fixture.scene.booted.database.acquire().await?)
            .await?,
    )
}
