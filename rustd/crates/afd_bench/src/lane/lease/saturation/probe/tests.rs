//! What the probe records, over a pool and a view that answer as scripted.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::future::ready;
use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::{Pool, Sampler, watch};
use crate::abort::Abort;
use crate::error::{ErrorKind, Result};
use crate::lane::lease::saturation::requester::Shared;

/// The population a window closes at when a fake exhausts it.
const POPULATION: u64 = 1;

/// A window long enough that only a fake closes it.
const WINDOW: Duration = Duration::from_secs(60);

fn window() -> Shared {
    Shared {
        deadline: Instant::now() + WINDOW,
        leased: AtomicU64::new(0),
        population: POPULATION,
        abort: Arc::new(Abort::new(1.0)),
    }
}

/// Answers `arrives` for `calls` acquires, then closes the window.
struct Closing<'a> {
    shared: &'a Shared,
    calls: AtomicU64,
    arrives: bool,
}

impl<'a> Closing<'a> {
    fn new(shared: &'a Shared, calls: u64, arrives: bool) -> Self {
        Self {
            shared,
            calls: AtomicU64::new(calls),
            arrives,
        }
    }
}

impl Pool for Closing<'_> {
    fn acquire(&self) -> impl Future<Output = bool> + Send {
        if self.calls.fetch_sub(1, Ordering::Relaxed) <= 1 {
            self.shared.leased.store(POPULATION, Ordering::Relaxed);
        }
        ready(self.arrives)
    }

    fn serves(&self, _count: u32) -> impl Future<Output = bool> + Send {
        // The probe never asks for recovery; a fake that answered would only
        // be proving itself.
        ready(false)
    }
}

/// Answers the same ages every sample, or refuses when `ages` is `None`.
struct Fixed(Option<Vec<f64>>);

impl Sampler for Fixed {
    fn ages(&mut self) -> impl Future<Output = Result<Vec<f64>>> + Send {
        let ages = self.0.clone();
        ready(ages.ok_or_else(|| ErrorKind::TaskLost { role: "view" }.into()))
    }
}

#[tokio::test]
async fn test_the_probe_times_every_acquire_until_the_window_closes() {
    let shared = window();
    let pool = Closing::new(&shared, 3, true);
    let watched = watch(&pool, &mut Fixed(Some(vec![5.0])), &shared)
        .await
        .expect("a scripted window is watched");
    assert_eq!(watched.wait.count(), 3);
    assert_eq!(watched.refused, 0);
    assert!(!watched.age.is_empty(), "open transactions were sampled");
}

#[tokio::test]
async fn test_an_acquire_past_its_budget_is_counted_refused() {
    let shared = window();
    let pool = Closing::new(&shared, 2, false);
    let watched = watch(&pool, &mut Fixed(Some(Vec::new())), &shared)
        .await
        .expect("refusals are counted");
    assert_eq!((watched.wait.count(), watched.refused), (0, 2));
    assert!(watched.age.is_empty(), "no transaction was open");
}

#[tokio::test]
async fn test_a_clock_skewed_negative_age_is_recorded_as_zero() {
    let shared = window();
    // Two acquires: `join!` polls the probe first, and a window closed by
    // its first acquire would end before the sampler ever read.
    let pool = Closing::new(&shared, 2, true);
    let watched = watch(&pool, &mut Fixed(Some(vec![-3.0])), &shared)
        .await
        .expect("a negative age is clamped, not refused");
    assert!(!watched.age.is_empty(), "the age was recorded");
    assert_eq!(watched.age.max_ms(), 0.0);
}

#[tokio::test]
async fn test_a_view_that_will_not_answer_refuses_the_watch() {
    let shared = window();
    let pool = Closing::new(&shared, 2, true);
    watch(&pool, &mut Fixed(None), &shared)
        .await
        .expect_err("an unanswered view ends the watch");
}
