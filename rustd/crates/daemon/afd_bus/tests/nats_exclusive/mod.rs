//! Tests that stop a real node, so they run alone (the lane's
//! `EXCLUSIVE_MODULES`).
//!
//! Stopping `nats-2` drops every client on it. A sibling test whose seed or
//! publisher sat there would fail for a reason that is not its own.

use std::time::Duration;

use af_core::env::{EnvSource as _, ProcessEnv};
use afd_bus::{Handle, Received, Subject, Subscription, nats::Tuning};
use tokio::time::Instant;

use super::{
    fleet,
    support::{Fallible, Proxy, TestResult, bus, namespace, port_of, seeds, settle, until, via},
};

/// The command line `make` exports for driving the compose cluster; the test
/// appends the verb.
const CONTROL_ENV: &str = "TEST_NATS_CONTROL";
/// The node every test here stops.
const VICTIM: &str = "nats-2";
/// The deadline a viewer on a stopped node resumes inside: its client fails
/// over to a live seed, and the viewer is told of the gap before it does.
const RESUME: Duration = Duration::from_secs(10);

/// `nats-2`, stopped until [`Self::restart`], or until this drops on a failed
/// test, so later suites find three nodes either way.
struct Stopped {
    up: bool,
}

impl Stopped {
    fn node() -> Fallible<Self> {
        control(&format!("stop {VICTIM}"))?;
        Ok(Self { up: false })
    }

    fn restart(mut self) -> TestResult {
        self.up = true;
        control(&format!("up -d --wait {VICTIM}"))
    }
}

impl Drop for Stopped {
    fn drop(&mut self) {
        if !self.up {
            // The test already failed; a restart error here adds nothing.
            let _ = control(&format!("up -d --wait {VICTIM}"));
        }
    }
}

fn control(verb: &str) -> TestResult {
    let prefix = ProcessEnv
        .get(CONTROL_ENV)
        .ok_or("TEST_NATS_CONTROL is unset: run through make test-integration-rustd")?;
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{prefix} {verb}"))
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`{verb}` failed: {stderr}").into());
    }
    Ok(())
}

/// One seed by its node's index.
fn node(seeds: &[String], index: usize) -> Fallible<&str> {
    Ok(seeds.get(index).ok_or("the lane exports three seeds")?)
}

/// A client on `nats-2` that can still fail over.
///
/// Its second seed relays to `nats-1` and refuses while the client boots, so
/// boot lands on `nats-2`; the relay opens once the client is up. The client
/// shuffles its seeds, so without the relay the node it dials is a coin flip.
async fn on_the_victim(seeds: &[String], namespace: &str) -> Fallible<(Handle, Proxy)> {
    let peer = node(seeds, 0)?;
    let relay = Proxy::start(port_of(peer)?).await?;
    relay.cut().await;
    let pair = format!("{},{}", node(seeds, 1)?, via(peer, relay.port())?);
    let handle = bus(&pair, namespace, Tuning::default()).await?;
    relay.restore();
    Ok((handle, relay))
}

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn node_kill_gaps_viewers_and_resumes() -> TestResult {
    let seeds = seeds()?;
    let ns = namespace();
    let subject = fleet(41)?;
    let publisher = bus(node(&seeds, 2)?, &ns, Tuning::default()).await?;
    let (victim, _relay) = on_the_victim(&seeds, &ns).await?;
    let survivor = bus(node(&seeds, 0)?, &ns, Tuning::default()).await?;
    let mut stranded = victim.subscribe(&subject).await?;
    let mut beside = survivor.subscribe(&subject).await?;
    let mut local = publisher.subscribe(&subject).await?;
    settle(
        &publisher,
        &subject,
        &mut [&mut stranded, &mut beside, &mut local],
    )
    .await?;

    let stopped = Stopped::node()?;
    let before = tokio::time::timeout(RESUME, until(&publisher, &subject, &mut stranded, "after"))
        .await
        .map_err(|_elapsed| {
            format!("the viewer on the stopped node did not resume inside {RESUME:?}")
        })??;
    // `until` returns what arrived before the resumed frame, so a gap in it is
    // a gap told before the viewer resumed.
    assert!(
        before.contains(&Received::Gap),
        "the viewer on the stopped node is told before it resumes: {before:?}"
    );
    for viewer in [&mut beside, &mut local] {
        let seen = until(&publisher, &subject, viewer, "after").await?;
        assert!(
            !seen.contains(&Received::Gap),
            "a viewer on a live node loses nothing: {seen:?}"
        );
    }

    stopped.restart()?;
    publisher.shutdown().await?;
    victim.shutdown().await?;
    survivor.shutdown().await?;
    Ok(())
}

/// Frames the loss test publishes, one every [`PACE`].
const FRAMES: u32 = 1_000;
const PACE: Duration = Duration::from_millis(10);
/// The frame before which `nats-2` is told to stop.
const STOP_BEFORE: u32 = 200;
/// How long a reader waits for the last frame.
const READ_LIMIT: Duration = Duration::from_secs(30);

#[tokio::test]
#[ignore = "needs live NATS: make test-integration-rustd"]
async fn every_lost_frame_follows_a_gap() -> TestResult {
    let seeds = seeds()?;
    let ns = namespace();
    let subject = fleet(42)?;
    let publisher = bus(node(&seeds, 0)?, &ns, Tuning::default()).await?;
    let (victim, _relay) = on_the_victim(&seeds, &ns).await?;
    let third = bus(node(&seeds, 2)?, &ns, Tuning::default()).await?;
    let mut stranded = victim.subscribe(&subject).await?;
    let mut far = third.subscribe(&subject).await?;
    let mut local = publisher.subscribe(&subject).await?;
    settle(
        &publisher,
        &subject,
        &mut [&mut stranded, &mut far, &mut local],
    )
    .await?;
    let readers = [stranded, far, local].map(|subscription| tokio::spawn(read_all(subscription)));

    let mut stop = None;
    for frame in 1..=FRAMES {
        if frame == STOP_BEFORE {
            stop = Some(tokio::task::spawn_blocking(Stopped::node));
        }
        publisher.publish(&subject, &frame.to_string()).await?;
        tokio::time::sleep(PACE).await;
    }
    let stopped = stop.ok_or("the stop was issued")?.await??;

    let mut stranded_told = false;
    for (index, reader) in readers.into_iter().enumerate() {
        let seen = reader.await?;
        let last = numbered(&seen).last().copied();
        assert_eq!(last, Some(FRAMES), "reader {index} reached the end");
        let holes = unannounced_holes(&seen);
        assert!(
            holes.is_empty(),
            "reader {index} lost frames with no signal: {holes:?}"
        );
        stranded_told |= index == 0 && seen.contains(&Received::Gap);
    }
    assert!(stranded_told, "the stranded reader was told at least once");

    stopped.restart()?;
    publisher.shutdown().await?;
    victim.shutdown().await?;
    third.shutdown().await?;
    Ok(())
}

/// Everything a subscription delivers until the last frame or the limit.
async fn read_all(mut subscription: Subscription) -> Vec<Received> {
    let deadline = Instant::now() + READ_LIMIT;
    let mut seen = Vec::new();
    while let Ok(Ok(received)) = tokio::time::timeout_at(deadline, subscription.recv()).await {
        let last = matches!(&received, Received::Message(message) if *message.payload == FRAMES.to_string());
        seen.push(received);
        if last {
            break;
        }
    }
    seen
}

fn number(received: &Received) -> Option<u32> {
    match received {
        Received::Message(message) => message.payload.parse().ok(),
        Received::Gap | Received::Lagged(_) => None,
    }
}

fn numbered(seen: &[Received]) -> Vec<u32> {
    seen.iter().filter_map(number).collect()
}

/// Each `(last seen, next seen)` jump that no `Gap` or `Lagged` came before.
fn unannounced_holes(seen: &[Received]) -> Vec<(u32, u32)> {
    let mut last = 0;
    let mut told = false;
    let mut holes = Vec::new();
    for received in seen {
        let Some(frame) = number(received) else {
            told |= matches!(received, Received::Gap | Received::Lagged(_));
            continue;
        };
        if frame > last + 1 && !told {
            holes.push((last, frame));
        }
        last = last.max(frame);
        told = false;
    }
    holes
}

#[test]
fn a_hole_counts_only_without_a_signal_before_it() {
    let frame = |n: u32| {
        Received::Message(std::sync::Arc::new(afd_bus::Message {
            subject: Subject::OutboundWake,
            payload: std::sync::Arc::new(n.to_string()),
        }))
    };
    let seen = [
        frame(1),
        frame(2),
        Received::Gap,
        frame(5),
        frame(9),
        frame(10),
    ];
    assert_eq!(unannounced_holes(&seen), [(5, 9)]);
}
