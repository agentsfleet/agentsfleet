//! What a host counts, renews and stops on, over a scripted path.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::future::ready;
use core::time::Duration;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use afd_core::id::Uuid7;

use super::{Held, Host, Polled, Shared, Tally, drive};
use crate::abort::Abort;
use crate::error::Result;

/// A window long enough that only the population ends it.
const WINDOW: Duration = Duration::from_secs(60);

/// An abort threshold no scripted run reaches.
const NEVER: f64 = 1.0;

/// Why a fixture identifier parses.
const CANONICAL: &str = "canonical v7";

fn runner() -> Uuid7 {
    Uuid7::parse("01920000-0000-7000-8000-000000000001").expect(CANONICAL)
}

fn held(fence: &str) -> Held {
    Held {
        lease: Uuid7::parse("01920000-0000-7000-8000-000000000002").expect(CANONICAL),
        fence: fence.to_owned(),
    }
}

fn shared(population: u64, deadline: Instant) -> Shared {
    Shared {
        deadline,
        leased: AtomicU64::new(0),
        population,
        abort: Arc::new(Abort::new(NEVER)),
    }
}

/// Answers its script in order, then nothing; renews as `renews` says.
struct Scripted {
    script: Mutex<VecDeque<Polled>>,
    renews: bool,
    renewed: AtomicU64,
}

impl Scripted {
    fn new(script: Vec<Polled>, renews: bool) -> Self {
        Self {
            script: Mutex::new(script.into()),
            renews,
            renewed: AtomicU64::new(0),
        }
    }
}

impl Host for Scripted {
    fn poll(&self, _runner: &Uuid7) -> impl Future<Output = Result<Polled>> + Send {
        let next = self.script.lock().expect("unpoisoned").pop_front();
        ready(Ok(next.unwrap_or(Polled::Nothing)))
    }

    fn renew(&self, _runner: &Uuid7, _held: &Held) -> impl Future<Output = bool> + Send {
        self.renewed.fetch_add(1, Ordering::Relaxed);
        ready(self.renews)
    }
}

async fn run(host: &Scripted, window: &Shared) -> Tally {
    drive(host, &runner(), window)
        .await
        .expect("a scripted window drives")
}

#[tokio::test]
async fn test_a_host_counts_each_kind_and_stops_at_the_population() {
    let host = Scripted::new(
        vec![
            Polled::Chat(held("1")),
            Polled::Nothing,
            Polled::Refused,
            Polled::Fleet,
        ],
        true,
    );
    let window = shared(2, Instant::now() + WINDOW);
    let tally = run(&host, &window).await;
    assert_eq!((tally.chat, tally.fleet, tally.polls), (1, 1, 4));
    assert_eq!(tally.failures, 1, "the refusal is counted");
    assert_eq!(tally.lease_latency.count(), 2, "only leases are timed");
    assert_eq!(window.leased.load(Ordering::Relaxed), 2);
    assert!(!window.open(), "an exhausted population closes the window");
}

#[tokio::test]
async fn test_a_host_renews_what_it_holds_every_round() {
    let mut script = vec![Polled::Chat(held("1")), Polled::Chat(held("1"))];
    script.extend([Polled::Nothing, Polled::Nothing, Polled::Nothing]);
    script.extend([Polled::Nothing, Polled::Nothing, Polled::Fleet]);
    let host = Scripted::new(script, true);
    let tally = run(&host, &shared(3, Instant::now() + WINDOW)).await;
    // Rounds fire before polls 4 and 8, each over the two chat leases held.
    assert_eq!(host.renewed.load(Ordering::Relaxed), 4);
    assert_eq!((tally.renewals, tally.failures), (4, 0));
}

#[tokio::test]
async fn test_a_refused_renewal_is_counted_not_propagated() {
    let mut script = vec![Polled::Chat(held("1"))];
    script.extend([Polled::Nothing, Polled::Nothing, Polled::Fleet]);
    let host = Scripted::new(script, false);
    let tally = run(&host, &shared(2, Instant::now() + WINDOW)).await;
    assert_eq!((tally.renewals, tally.failures), (0, 1));
}

#[tokio::test]
async fn test_a_passed_deadline_polls_nothing() {
    let host = Scripted::new(vec![Polled::Fleet], true);
    let tally = run(&host, &shared(1, Instant::now())).await;
    assert_eq!(tally.polls, 0);
}

#[tokio::test]
async fn test_tallies_absorb_every_count_and_sample() {
    let one = Scripted::new(vec![Polled::Chat(held("1")), Polled::Refused], true);
    let other = Scripted::new(vec![Polled::Fleet], true);
    let mut total = run(&one, &shared(1, Instant::now() + WINDOW)).await;
    let theirs = run(&other, &shared(1, Instant::now() + WINDOW)).await;
    total.absorb(&theirs).expect("latencies merge");
    assert_eq!((total.polls, total.chat, total.fleet), (2, 1, 1));
    assert_eq!(total.lease_latency.count(), 2);
}
