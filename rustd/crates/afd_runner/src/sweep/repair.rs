//! Turning a durable repair-verification intent into a fleet event.
//!
//! When a repair this daemon proposed is merged and then deployed, something
//! has to go and check whether the incident actually stopped. That check is an
//! ordinary fleet run, and this sweeper is what starts it — after a settling
//! window, so the deployment has had time to take effect.
//!
//! # Why an intent is durable and the dispatch is not
//!
//! The two writes cannot be one transaction: appending the event is Redis and
//! recording that it happened is Postgres. So the intent is written durably
//! first, and this loop retries it until the database records which event it
//! produced. What makes the retry safe is that the append is `append_once` —
//! a second attempt returns the FIRST attempt's event id rather than appending
//! a second event. A duplicate here is not a tidiness problem: it is the same
//! verification running twice, with real provider spend.
//!
//! # A claim that lapses is a claim that is released
//!
//! Every replica runs this. A pass claims a batch with a token and a timestamp,
//! and a claim older than [`CLAIM_STALE`] is available again — so a dispatcher
//! that died mid-flight releases its work by ELAPSING, with nothing having to
//! notice it died. The completion is guarded on the token, so the dead pass
//! cannot come back and overwrite what its replacement recorded.
//!
//! # The three pacings
//!
//! A full batch means more work is waiting, so the next pass follows
//! immediately. A batch with failures in it waits a claim's lifetime, because
//! that is when the failed rows become claimable again and coming back sooner
//! would find them still held. Anything else waits the ordinary interval.

use std::sync::Mutex;
use std::time::Duration;

use afd_core::clock::{self, UnixMillis};
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::metrics::label::fleet::{SyntheticEvent, VerifierRun};
use afd_observability::producers;
use afd_redis::Redis;
use afd_redis::streams::{FleetStreams, OnceScope};
use afd_wire::event::EventType;
use sqlx::Row as _;

mod cleanup;
mod rows;

use self::rows::{Dispatched, Due, Incident, Production, Repair, Synthetic};
use crate::error::{Result, query};
use crate::sql;
use crate::sweep::{Sweep, Swept};

/// Statement name, for the context a query failure carries.
const CONTEXT_CLAIM: &str = "repair verification claim";

/// Statement name, for the context a query failure carries.
const CONTEXT_COMPLETE: &str = "repair verification completion";

/// Statement name, for the context a query failure carries.
const CONTEXT_CLEANUP: &str = "repair verification cleanup";

/// Milliseconds per second, for the one conversion this module performs.
const MILLIS_PER_SECOND: i64 = 1_000;

/// How many intents one pass claims.
const DUE_BATCH_LIMIT: i64 = 32;

/// How many append-once keys one pass forgets.
const CLEANUP_BATCH_LIMIT: i64 = 32;

/// How long a claim holds an intent before another pass may take it.
const CLAIM_STALE: Duration = Duration::from_secs(30);

/// How long between ordinary passes.
const INTERVAL: Duration = Duration::from_mins(1);

/// How long a pass with no work at all waits before looking again.
const NOTHING_DUE_INTERVAL: Duration = INTERVAL;

/// The `event_type` a verification run is triggered as.
///
/// The same word an inbound webhook carries, because that is what this IS from
/// the fleet's side: something happened outside, and the fleet is being asked
/// to look at it. A type of its own would need a matching arm in every
/// trigger-matching path for a distinction the fleet does not act on.
const TRIGGER_EVENT_TYPE: EventType = EventType::Webhook;

/// What a verification event's payload calls itself.
const SYNTHETIC_EVENT: &str = "repair_production_result";

/// Who the event records as its author.
///
/// Prefixed `system:` like every other non-human actor, so an audit reading a
/// fleet's history can tell a daemon-authored run from a person's.
pub const VERIFIER_ACTOR: &str = "system:repair-verifier";

/// One intent this pass claimed.
///
/// Every field rides into the event payload except the identifiers, which are
/// what the completion is written against.
/// The repair-verification dispatcher.
#[derive(Debug)]
pub struct Repairs {
    /// Where the intents are.
    database: Db,
    /// Where the events are appended.
    streams: FleetStreams,
    /// The claim tokens this pass writes.
    entropy: Entropy,
    /// What the last pass concluded about when to come back.
    pacing: Mutex<Duration>,
}

impl Repairs {
    /// A dispatcher over `database` and `queue`.
    #[must_use]
    pub fn new(database: Db, queue: Redis, entropy: Entropy) -> Self {
        Self {
            database,
            streams: FleetStreams::new(queue),
            entropy,
            pacing: Mutex::new(INTERVAL),
        }
    }

    /// Claims the intents whose wait is over.
    async fn claim(&self, now: UnixMillis, token: &Uuid7) -> Result<Vec<Due>> {
        let stale_before = now
            .as_millis()
            .saturating_sub(i64::try_from(CLAIM_STALE.as_millis()).unwrap_or(i64::MAX));
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::CLAIM_DUE_REPAIR_VERIFICATIONS)
            .bind(now.as_millis())
            .bind(stale_before)
            .bind(DUE_BATCH_LIMIT)
            .bind(token.as_str())
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLAIM))?;

        rows.iter()
            .map(|row| {
                let read = |index: usize| {
                    row.try_get::<String, _>(index)
                        .map_err(query(CONTEXT_CLAIM))
                };
                let number =
                    |index: usize| row.try_get::<i64, _>(index).map_err(query(CONTEXT_CLAIM));
                Ok(Due {
                    id: read(0)?,
                    verifier_fleet_id: read(4)?,
                    workspace_id: read(3)?,
                    incident: Incident {
                        fleet_id: read(5)?,
                        event_id: read(6)?,
                        request_json: read(7)?,
                        response_text: read(8)?,
                    },
                    repair: Repair {
                        pr_number: number(9)?,
                        pr_url: read(10)?,
                        merged_commit_sha: read(11)?,
                        merged_at: number(12)?,
                    },
                    production: Production {
                        provider: read(13)?,
                        deployment_id: read(14)?,
                        conclusion: read(15)?,
                        completed_at: number(16)?,
                    },
                    verify_after: number(17)?,
                })
            })
            .collect()
    }

    /// Appends one intent's event and records that it did.
    ///
    /// The append is idempotent and the completion is guarded, so every failure
    /// mode here leaves the intent claimable again rather than half-done: an
    /// append that succeeded and a completion that did not is retried, and the
    /// retry returns the same event id rather than a second event.
    async fn dispatch(&self, intent: &Due, token: &Uuid7, now: UnixMillis) -> Result<bool> {
        let payload = serde_json::to_string(&Synthetic {
            event_type: SYNTHETIC_EVENT,
            incident: &intent.incident,
            repair: &intent.repair,
            production: &intent.production,
        })
        .map_err(|_shape| crate::error::vault_data_invalid())?;

        let created_at = now.as_millis().to_string();
        let appended = self
            .streams
            .append_once(
                OnceScope::FleetIntent,
                &intent.id,
                &intent.verifier_fleet_id,
                &[
                    ("fleet_id", intent.verifier_fleet_id.as_str()),
                    ("workspace_id", intent.workspace_id.as_str()),
                    ("actor", VERIFIER_ACTOR),
                    ("event_type", TRIGGER_EVENT_TYPE.as_str()),
                    ("request_json", payload.as_str()),
                    ("created_at", created_at.as_str()),
                ],
            )
            .await?;

        producers::fleet::repair::event(if appended.replayed {
            SyntheticEvent::Replayed
        } else {
            SyntheticEvent::Emitted
        });
        producers::fleet::repair::run(VerifierRun::Queued);

        let mut connection = self.database.acquire().await?;
        let recorded = sqlx::query(sql::sweep::COMPLETE_REPAIR_VERIFICATION)
            .bind(intent.id.as_str())
            .bind(token.as_str())
            .bind(appended.id.as_str())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_COMPLETE))?;
        Ok(recorded.rows_affected() > 0)
    }

    /// A fresh claim token for one pass.
    fn token(&self, now: UnixMillis) -> Result<Uuid7> {
        let mut bytes = [0u8; ENTROPY_LEN];
        self.entropy.fill(&mut bytes)?;
        Ok(Uuid7::encode(now, bytes)?)
    }
}

impl Sweep for Repairs {
    fn name(&self) -> &'static str {
        "repair-verification"
    }

    /// What the last pass concluded — see [`Dispatched::pacing`].
    fn interval(&self) -> Duration {
        self.pacing.lock().map_or(INTERVAL, |pacing| *pacing)
    }

    async fn sweep(&self) -> Result<Swept> {
        let now = clock::now();
        let token = self.token(now)?;
        let due = self.claim(now, &token).await?;
        observe_backlog(&due, now);
        let mut dispatched = Dispatched {
            due: due.len(),
            ..Dispatched::default()
        };

        for intent in &due {
            match self.dispatch(intent, &token, now).await {
                Ok(true) => {
                    dispatched.completed += 1;
                    producers::fleet::repair::run(VerifierRun::Completed);
                }
                // The append happened and the completion did not match — this
                // pass's claim had already lapsed and another replica recorded
                // it. Counted as failed so the pacing waits, and correct
                // either way: the event exists exactly once.
                Ok(false) => {
                    dispatched.failed += 1;
                    producers::fleet::repair::dispatch_retried();
                }
                Err(failure) => {
                    dispatched.failed += 1;
                    producers::fleet::repair::dispatch_retried();
                    tracing::warn!(
                        verification_id = intent.id,
                        workspace_id = intent.workspace_id,
                        due_since_ms = now.as_millis().saturating_sub(intent.verify_after),
                        error = %failure,
                        event = "repair_verification_dispatch_failed",
                        "an intent could not be dispatched; its claim will lapse and it retries"
                    );
                }
            }
        }

        dispatched.cleanup_pending = self.clean(now).await;
        if let Ok(mut pacing) = self.pacing.lock() {
            *pacing = dispatched.pacing();
        }
        Ok(Swept {
            scanned: u64::try_from(dispatched.due).unwrap_or(u64::MAX),
            changed: u64::try_from(dispatched.completed).unwrap_or(u64::MAX),
        })
    }
}

#[cfg(test)]
mod tests;

/// Publishes what this pass found waiting, for the two backlog gauges.
///
/// The oldest intent's age rather than the newest's: a queue that is moving
/// has a young head and a stalled one does not, and the batch size alone
/// cannot tell those apart — a full batch is what a healthy busy pass and a
/// wedged one both look like.
fn observe_backlog(due: &[Due], now: UnixMillis) {
    let oldest = due
        .iter()
        .map(|intent| now.as_millis().saturating_sub(intent.verify_after))
        .max()
        .unwrap_or_default();
    producers::fleet::repair_backlog_observed(
        u64::try_from(due.len()).unwrap_or(u64::MAX),
        u64::try_from(oldest / MILLIS_PER_SECOND).unwrap_or_default(),
    );
}
