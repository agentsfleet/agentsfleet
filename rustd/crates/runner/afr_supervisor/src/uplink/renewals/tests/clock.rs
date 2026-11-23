//! The runner's one clock, on a paused runtime: when each round falls, what it
//! sends, and how the interval and a refusal move the next one.

use std::{sync::Arc, time::Duration};

use af_core::id::Uuid7;
use af_wire::protocol::Up;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::{super::Renewals, RenewalPeer};

const TEN: Duration = Duration::from_secs(10);
const SEVEN: Duration = Duration::from_secs(7);
const FIVE: Duration = af_core::timing::STREAM_ANSWER_TIMEOUT;

/// The next renewal batch: when it was sent, from `started`, and its request id.
async fn next_batch(
    peer: &mut RenewalPeer,
    started: Instant,
) -> crate::Result<(Duration, std::num::NonZeroU64, String)> {
    let message = peer
        .outgoing
        .priority
        .recv()
        .await
        .ok_or(crate::error::ErrorKind::Disconnected)?;
    let Up::RenewBatch { request_id, runs } = message.frame.decode::<Up<'_>>()? else {
        return Err(crate::error::ErrorKind::StreamProtocol.into());
    };
    let run = runs
        .first()
        .map(|renewal| renewal.run_id.to_string())
        .ok_or(crate::error::ErrorKind::StreamProtocol)?;
    Ok((started.elapsed(), request_id, run))
}

/// Answers the batch `request_id` for `run`: accepted, or refused for now.
async fn answer(
    peer: &RenewalPeer,
    request_id: std::num::NonZeroU64,
    run: &str,
    accepted: bool,
) -> crate::Result<()> {
    let answer = if accepted {
        serde_json::json!({"status":"ok","response":{"run_expires_at":"2030-01-01T00:00:00Z"}})
    } else {
        serde_json::json!({"status":"refused","error_code":"UZ-RUN-027",
            "detail":"Unavailable","retryable":true})
    };
    peer.deliver(
        &serde_json::json!({"type":"renewed","request_id":request_id,
        "runs":[{"run_id":run,"answer":answer}]}),
    )
    .await
}

/// Whether a beat is waiting for the heartbeat loop right now.
async fn beat_waiting(registry: &Renewals) -> bool {
    tokio::time::timeout(Duration::ZERO, registry.beats().notified())
        .await
        .is_ok()
}

fn held(registry: &Arc<Renewals>) -> crate::Result<super::super::Registration> {
    let run = Uuid7::parse(crate::test_util::RUN_ID)?;
    registry.register(
        &run,
        Arc::new(afr_agent::Meter::default()),
        Instant::now() + Duration::from_secs(60),
    )
}

/// Each round wakes the beat and renews at once, one interval after the last; an
/// interval advertised later re-arms the next round from the last one sent.
#[tokio::test(start_paused = true)]
async fn each_round_wakes_the_beat_and_follows_a_newly_advertised_interval() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let _held = held(&registry)?;
    registry.advertised(TEN);
    let started = Instant::now();
    let stop = CancellationToken::new();
    let running = registry.run(&handle, stop.clone());
    let observed = async {
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(at, TEN, "the first round is one interval in");
        assert!(beat_waiting(&registry).await, "the round woke the beat");
        answer(&peer, request, &run, true).await?;
        registry.advertised(SEVEN);
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(
            at,
            TEN + SEVEN,
            "the new interval counts from the last round"
        );
        answer(&peer, request, &run, true).await?;
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}

/// A round refused for now is followed one answer bound after it was sent, and
/// the round after an acceptance one interval after that; the run stays held.
#[tokio::test(start_paused = true)]
async fn a_refused_renewal_is_retried_one_answer_bound_after_it_was_sent() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let held = held(&registry)?;
    registry.advertised(TEN);
    let started = Instant::now();
    let stop = CancellationToken::new();
    let running = registry.run(&handle, stop.clone());
    let observed = async {
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(at, TEN);
        answer(&peer, request, &run, false).await?;
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(
            at,
            TEN + FIVE,
            "retried one answer bound after the refusal's round"
        );
        assert!(beat_waiting(&registry).await, "a retry round beats too");
        answer(&peer, request, &run, true).await?;
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(
            at,
            TEN + FIVE + TEN,
            "an accepted round waits a whole interval"
        );
        answer(&peer, request, &run, true).await?;
        assert!(held.updates.borrow().is_some(), "the run is still held");
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}

/// With no run held a round still wakes the beat and sends no renewal; rounds
/// nobody took leave one beat waiting, not one per round.
#[tokio::test(start_paused = true)]
async fn an_idle_round_beats_alone_and_a_waiting_beat_is_stored_once() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    registry.advertised(TEN);
    let stop = CancellationToken::new();
    let running = tokio::spawn({
        let registry = Arc::clone(&registry);
        let stop = stop.clone();
        async move { registry.run(&handle, stop).await }
    });
    tokio::time::sleep(TEN * 2 + Duration::from_millis(1)).await;
    assert!(
        peer.outgoing.priority.try_recv().is_err(),
        "no run held, no renewal sent"
    );
    assert!(beat_waiting(&registry).await, "the rounds woke the beat");
    assert!(
        !beat_waiting(&registry).await,
        "two rounds nobody took leave one beat, not two"
    );
    stop.cancel();
    running.await.map_err(crate::error::stream_protocol)??;
    Ok(())
}

/// A run in flight renews on the clock whether or not a beat is taken, so the
/// heartbeat loop's last beat at shutdown leaves its runs renewed.
#[tokio::test(start_paused = true)]
async fn a_run_in_flight_renews_after_the_last_beat() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let _held = held(&registry)?;
    registry.advertised(TEN);
    let started = Instant::now();
    let stop = CancellationToken::new();
    let running = registry.run(&handle, stop.clone());
    let observed = async {
        for round in 1..=3_u32 {
            let (at, request, run) = next_batch(&mut peer, started).await?;
            assert_eq!(at, TEN * round, "round {round} renews on time");
            answer(&peer, request, &run, true).await?;
        }
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}

/// A retry pending when a longer interval is advertised keeps its own due time, even
/// when the old interval equalled the retry bound and the two cannot be told apart by
/// their instants.
#[tokio::test(start_paused = true)]
async fn a_pending_retry_keeps_its_time_when_a_longer_interval_arrives() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let _held = held(&registry)?;
    registry.advertised(FIVE);
    let started = Instant::now();
    let stop = CancellationToken::new();
    let running = registry.run(&handle, stop.clone());
    let observed = async {
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(at, FIVE);
        answer(&peer, request, &run, false).await?;
        registry.advertised(TEN);
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(
            at,
            FIVE + FIVE,
            "the retry is not pushed to the new interval"
        );
        answer(&peer, request, &run, true).await?;
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(
            at,
            FIVE + FIVE + TEN,
            "the round after it follows the new interval"
        );
        answer(&peer, request, &run, true).await?;
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}

/// A pending retry moves up when a shorter interval is advertised: one interval when
/// that is shorter than the answer bound.
#[tokio::test(start_paused = true)]
async fn a_pending_retry_moves_up_to_a_shorter_interval() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let _held = held(&registry)?;
    registry.advertised(TEN);
    let started = Instant::now();
    let stop = CancellationToken::new();
    let running = registry.run(&handle, stop.clone());
    let three = Duration::from_secs(3);
    let observed = async {
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(at, TEN);
        answer(&peer, request, &run, false).await?;
        registry.advertised(three);
        let (at, request, run) = next_batch(&mut peer, started).await?;
        assert_eq!(at, TEN + three, "the retry follows the shorter interval");
        answer(&peer, request, &run, true).await?;
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}
