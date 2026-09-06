//! What one delivery worker sustains, and what a slow destination costs the
//! jobs queued behind it.
//!
//! # The shape of a run
//!
//! Queue N jobs across D destinations, then start the real `Worker` and let it
//! drain them. The worker is one task reading one entry at a time and awaiting
//! its retry ladder inline, so every job's delivery latency includes the time
//! every earlier job spent — which is exactly the head-of-line cost this lane
//! exists to measure. Nothing here changes that shape; the numbers are what
//! say whether it should change.
//!
//! # Three populations, one queue
//!
//! Destinations are scripted fast, slow, or retryable by fraction. A slow one
//! answers late; a retryable one never answers, so the worker walks its ladder
//! and gives up. The report separates the latency of the OTHER jobs from the
//! slow ones, and the fraction of the window the ladder held the worker.

pub mod poster;

use core::time::Duration;
use std::collections::HashMap;
use std::time::Instant;

use afd_outbound::{Posters, Worker};
use afd_redis::{Dedicated, OutboundJob, OutboundQueue, OutboundReader, outbound_consumer};
use tokio_util::sync::CancellationToken;

use self::poster::{Behaviour, Scripted};
use crate::datastores::{Datastores, redis_calls};
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::{Parameter, Profile};
use crate::report::{
    DatastoreCost, Datastores as ReportDatastores, Fixture, Lane, Latency, Report,
};

/// Measurement key: jobs the worker reached a terminal verdict on.
const DELIVERED: &str = "delivered";

/// Measurement key: p95 delivery latency of jobs to destinations that were
/// NOT scripted slow — the head-of-line cost, isolated.
const OTHERS_P95_MS: &str = "others_p95_ms";

/// Measurement key: p95 delivery latency of the slow destinations themselves.
const SLOW_P95_MS: &str = "slow_p95_ms";

/// Measurement key: fraction of the window the worker sat in its retry ladder.
const RETRY_OCCUPANCY: &str = "retry_occupancy";

/// Measurement key: how many destinations were scripted slow.
const SLOW_DESTINATIONS: &str = "slow_destinations";

/// A healthy vendor's answer time. One millisecond is well under the ladder's
/// first rung, so the fast population cannot be confused with a retry.
const FAST_ANSWER: Duration = Duration::from_millis(1);

/// A slow vendor's answer time: long enough that the jobs behind it visibly
/// wait, short enough that a window of seconds still drains.
const SLOW_ANSWER: Duration = Duration::from_millis(250);

/// How many destinations the jobs are spread over.
const DESTINATIONS: u64 = 16;

/// The provider every queued job names, so the worker routes it to the one
/// poster this build ships.
const PROVIDER: &str = "slack";

/// A generated answer; never a tenant's text (RULE PRI).
const ANSWER: &str = "bench answer";

/// How long to wait for the worker to stop after cancellation.
const STOP_GRACE: Duration = Duration::from_secs(6);

/// What the caller asked this lane to measure.
#[derive(Debug, Clone, Copy)]
pub struct Parameters {
    /// Jobs to queue.
    pub jobs: u64,
    /// Fraction of destinations scripted slow, in `0..=1`.
    pub slow_fraction: f64,
    /// Fraction of destinations scripted retryable, in `0..=1`.
    pub retryable_fraction: f64,
    /// The longest the drain may run.
    pub window: Duration,
}

impl Parameters {
    /// Refuse anything above the profile's ceiling, before a connection opens.
    ///
    /// # Errors
    ///
    /// [`crate::Error::CapExceeded`] naming the cap and the profile.
    pub fn admit(self, profile: Profile) -> Result<()> {
        profile.check(Parameter::Jobs, self.jobs)
    }
}

/// Run the lane and return the report it measured.
///
/// # Errors
///
/// A cap refusal, or a datastore that would not answer.
pub async fn run(
    profile: Profile,
    parameters: Parameters,
    stores: &Datastores,
    redis_url: &str,
    ca_cert: Option<String>,
    prefix: &RunPrefix,
) -> Result<Report> {
    parameters.admit(profile)?;
    let queue = OutboundQueue::new(stores.queue.clone());
    queue.ensure_group().await?;

    let behaviours = script(prefix, parameters);
    let poster = Scripted::new(behaviours.clone(), FAST_ANSWER, SLOW_ANSWER);
    let destinations: Vec<String> = behaviours.keys().cloned().collect();

    let mut ledger = FixtureLedger::new();
    let mut queued_at = HashMap::new();
    let mut round_robin = destinations.iter().cycle();
    for index in 0..parameters.jobs {
        let Some(destination) = round_robin.next() else {
            break;
        };
        let id = queue
            .enqueue(OutboundJob {
                provider: PROVIDER,
                workspace_id: prefix.as_str(),
                fleet_id: destination,
                event_id: &prefix.name(&format!("event-{index}")),
                answer: ANSWER,
            })
            .await?;
        queued_at.insert(id.as_str().to_owned(), Instant::now());
        ledger.created(1);
    }

    let drained = drain(
        stores,
        redis_url,
        ca_cert,
        queue,
        poster.clone(),
        parameters,
    )
    .await?;
    let mut report = Report::new(Lane::Outbound, profile);
    report.created = true;
    report.parameter(Parameter::Jobs.name(), parameters.jobs);
    record(&mut report, &drained, &poster, &queued_at, &behaviours)?;
    report.fixture = Fixture::of(prefix, ledger);
    Ok(report)
}

/// Assign a behaviour to each destination by the requested fractions.
fn script(prefix: &RunPrefix, parameters: Parameters) -> HashMap<String, Behaviour> {
    let slow = fraction_of(DESTINATIONS, parameters.slow_fraction);
    let retryable = fraction_of(DESTINATIONS, parameters.retryable_fraction);
    (0..DESTINATIONS)
        .map(|index| {
            let behaviour = if index < slow {
                Behaviour::Slow
            } else if index < slow + retryable {
                Behaviour::Retryable
            } else {
                Behaviour::Fast
            };
            (prefix.name(&format!("destination-{index}")), behaviour)
        })
        .collect()
}

/// How many of `total` a fraction selects, rounded down and clamped.
fn fraction_of(total: u64, fraction: f64) -> u64 {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sixteen destinations times a clamped fraction is a small non-negative integer"
    )]
    {
        (total as f64 * fraction.clamp(0.0, 1.0)).floor() as u64
    }
}

/// What the drain cost, either side of the worker's run.
struct Drained {
    elapsed: Duration,
    redis_calls: u64,
}

/// Start the real worker, wait for it to reach every job or the deadline, stop it.
async fn drain(
    stores: &Datastores,
    redis_url: &str,
    ca_cert: Option<String>,
    queue: OutboundQueue,
    poster: Scripted,
    parameters: Parameters,
) -> Result<Drained> {
    let config =
        afd_redis::RedisConfig::from_url(afd_redis::RedisRole::Default, redis_url.to_owned())
            .with_ca_cert_file(ca_cert.map(Into::into));
    let reader = OutboundReader::new(
        Dedicated::connect(&config, afd_outbound::LONGEST_PARK).await?,
        outbound_consumer(),
    );
    let redis_before = redis_calls(&stores.queue).await?;
    let token = CancellationToken::new();
    let started = Instant::now();
    let worker = tokio::spawn(
        Worker::new(
            reader,
            queue,
            Posters {
                slack: poster.clone(),
            },
        )
        .run(token.clone()),
    );

    // Terminal for the reporter means delivered OR the ladder exhausted, which
    // the poster sees as `DELIVERY_ATTEMPTS` attempts on one job.
    let deadline = started + parameters.window;
    while Instant::now() < deadline && settled(&poster) < parameters.jobs {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let elapsed = started.elapsed();
    token.cancel();
    let _ = tokio::time::timeout(STOP_GRACE, worker).await;
    Ok(Drained {
        elapsed,
        redis_calls: redis_calls(&stores.queue)
            .await?
            .saturating_sub(redis_before),
    })
}

/// Jobs the worker is finished with, one way or the other.
fn settled(poster: &Scripted) -> u64 {
    let seen = poster.seen();
    let exhausted = seen
        .attempts()
        .values()
        .filter(|attempts| attempts.len() >= afd_outbound::retry::DELIVERY_ATTEMPTS)
        .count();
    seen.terminal() + u64::try_from(exhausted).unwrap_or(u64::MAX)
}

/// Write the drain's numbers into the report.
fn record(
    report: &mut Report,
    drained: &Drained,
    poster: &Scripted,
    queued_at: &HashMap<String, Instant>,
    behaviours: &HashMap<String, Behaviour>,
) -> Result<()> {
    let seen = poster.seen();
    let mut all = Latency::new()?;
    let mut others = Latency::new()?;
    let mut slow = Latency::new()?;
    let mut ladder = Duration::ZERO;
    for (id, attempts) in seen.attempts() {
        let Some(first) = attempts.first() else {
            continue;
        };
        if let Some(queued) = queued_at.get(id) {
            let latency = first.at.saturating_duration_since(*queued);
            all.record(latency)?;
            match first.behaviour {
                Behaviour::Slow => slow.record(latency)?,
                Behaviour::Fast | Behaviour::Retryable => others.record(latency)?,
            }
        }
        for (earlier, later) in attempts.iter().zip(attempts.iter().skip(1)) {
            ladder += later.at.saturating_duration_since(earlier.at);
        }
    }
    let seconds = drained.elapsed.as_secs_f64();
    report.latency(seconds, &all);
    report.measurement(DELIVERED, count(seen.terminal()));
    // A population nothing landed in has no p95, and a zero would say the
    // slow destinations answered instantly on a run that scripted none.
    if !others.is_empty() {
        report.measurement(
            OTHERS_P95_MS,
            others.quantile_ms(crate::report::latency::P95),
        );
    }
    if !slow.is_empty() {
        report.measurement(SLOW_P95_MS, slow.quantile_ms(crate::report::latency::P95));
    }
    report.measurement(
        RETRY_OCCUPANCY,
        if seconds > 0.0 {
            ladder.as_secs_f64() / seconds
        } else {
            0.0
        },
    );
    report.measurement(
        SLOW_DESTINATIONS,
        count(
            behaviours
                .values()
                .filter(|b| **b == Behaviour::Slow)
                .count() as u64,
        ),
    );
    report.datastores = ReportDatastores {
        redis: DatastoreCost {
            operations: drained.redis_calls,
            time_ms: None,
        },
        // The scripted poster never opens Postgres, and neither does the
        // worker's loop: the only Postgres on this path is the real Slack
        // poster's destination read, which the script replaces.
        postgres: DatastoreCost {
            operations: 0,
            time_ms: None,
        },
    };
    Ok(())
}

/// A count as a ratio's operand.
fn count(value: u64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count past f64's exact range is not a run that finished"
    )]
    {
        value as f64
    }
}
