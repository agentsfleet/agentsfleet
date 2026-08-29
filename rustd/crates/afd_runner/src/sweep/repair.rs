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
use afd_redis::Redis;
use afd_redis::streams::{FleetStreams, OnceScope};
use afd_wire::event::EventType;
use serde::Serialize;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::sql;
use crate::sweep::{Sweep, Swept};

/// Statement name, for the context a query failure carries.
const CONTEXT_CLAIM: &str = "repair verification claim";

/// Statement name, for the context a query failure carries.
const CONTEXT_COMPLETE: &str = "repair verification completion";

/// Statement name, for the context a query failure carries.
const CONTEXT_CLEANUP: &str = "repair verification cleanup";

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
#[derive(Debug)]
struct Due {
    /// The verification row.
    id: String,
    /// The fleet that will RUN the verification.
    verifier_fleet_id: String,
    /// The workspace both fleets belong to.
    workspace_id: String,
    /// The incident's own fleet and event, which the payload names so the
    /// verification knows what it is checking.
    incident: Incident,
    /// The repair that was merged.
    repair: Repair,
    /// The deployment that carried it.
    production: Production,
    /// When this became due, for the pacing decision.
    verify_after: i64,
}

/// The incident a verification is checking.
#[derive(Debug, Serialize)]
struct Incident {
    /// The fleet the incident belonged to.
    fleet_id: String,
    /// The event that recorded it.
    event_id: String,
    /// What was asked, verbatim.
    request_json: String,
    /// What was answered.
    response_text: String,
}

/// The merged repair.
#[derive(Debug, Serialize)]
struct Repair {
    /// The pull request's number.
    pr_number: i64,
    /// Its address.
    pr_url: String,
    /// The commit the merge produced.
    merged_commit_sha: String,
    /// When it merged.
    merged_at: i64,
}

/// The deployment that carried the repair to production.
#[derive(Debug, Serialize)]
struct Production {
    /// Which deployment provider reported it.
    provider: String,
    /// Its identifier there.
    deployment_id: String,
    /// How it ended.
    conclusion: String,
    /// When it ended.
    completed_at: i64,
}

/// The payload a verification run is handed.
#[derive(Debug, Serialize)]
struct Synthetic<'a> {
    /// What this payload is.
    event_type: &'static str,
    /// The incident under verification.
    incident: &'a Incident,
    /// The repair that was supposed to fix it.
    repair: &'a Repair,
    /// The deployment that shipped it.
    production: &'a Production,
}

/// What one pass did, and what it implies about when to come back.
#[derive(Debug, Clone, Copy, Default)]
struct Dispatched {
    /// Intents claimed.
    due: usize,
    /// Intents that produced an event and were recorded.
    completed: usize,
    /// Intents that did not, and will be retried once their claim lapses.
    failed: usize,
    /// Whether the cleanup page was full, meaning more keys are waiting.
    cleanup_pending: bool,
}

impl Dispatched {
    /// How long to wait before the next pass.
    fn pacing(self) -> Duration {
        // A full cleanup page means more keys are waiting, and forgetting them
        // costs one round trip each — so the next pass follows immediately
        // rather than leaving keys in Redis for a minute at a time.
        if self.cleanup_pending {
            return Duration::ZERO;
        }
        let full_batch = self.due >= usize::try_from(DUE_BATCH_LIMIT).unwrap_or(usize::MAX);
        if self.failed > 0 && (!full_batch || self.completed == 0) {
            // Coming back sooner would find the failed rows still claimed by
            // this very pass, so the wait is exactly a claim's life.
            return CLAIM_STALE;
        }
        if full_batch {
            // A backlog: the next batch is already waiting.
            return Duration::ZERO;
        }
        NOTHING_DUE_INTERVAL
    }
}

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

    /// Forgets the append-once keys of intents that are fully recorded.
    ///
    /// Answers whether the page was full, which means more are waiting. A
    /// failure here is reported and swallowed: an un-forgotten key costs one
    /// Redis entry until the next pass, and failing the whole sweep over it
    /// would stop dispatching work that is due.
    async fn clean(&self, now: UnixMillis) -> bool {
        let Ok(page) = self.cleanup_page(now).await.inspect_err(|failure| {
            tracing::warn!(
                error = %failure,
                event = "repair_verification_cleanup_lookup_failed",
                "the append-once cleanup page could not be read"
            );
        }) else {
            return false;
        };
        if page.is_empty() {
            return false;
        }

        let mut forgotten = Vec::with_capacity(page.len());
        for id in &page {
            match self.streams.forget_once(OnceScope::FleetIntent, id).await {
                Ok(()) => forgotten.push(id.clone()),
                // Left for the next pass: the row keeps its uncleared marker,
                // so nothing is lost by not recording this one.
                Err(failure) => tracing::warn!(
                    verification_id = id,
                    error = %failure,
                    event = "repair_verification_once_key_uncleared",
                    "an append-once key could not be forgotten"
                ),
            }
        }
        if forgotten.is_empty() {
            return false;
        }

        let full = page.len() >= usize::try_from(CLEANUP_BATCH_LIMIT).unwrap_or(usize::MAX);
        if let Err(failure) = self.record_cleanup(&forgotten, now).await {
            tracing::warn!(
                error = %failure,
                event = "repair_verification_cleanup_update_failed",
                "forgotten append-once keys could not be recorded"
            );
            return false;
        }
        full
    }

    /// The intents whose keys are still in Redis.
    async fn cleanup_page(&self, now: UnixMillis) -> Result<Vec<String>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::SELECT_REPAIR_VERIFICATION_CLEANUP)
            .bind(now.as_millis())
            .bind(CLEANUP_BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLEANUP))?;
        rows.iter()
            .map(|row| row.try_get::<String, _>(0).map_err(query(CONTEXT_CLEANUP)))
            .collect()
    }

    /// Records that a batch of keys is gone.
    async fn record_cleanup(&self, forgotten: &[String], now: UnixMillis) -> Result<()> {
        // Serialised to TEXT and cast by the statement, because a `jsonb` bind
        // would need a sqlx feature this crate does not take for one array of
        // identifiers.
        let identifiers = serde_json::to_string(forgotten)
            .map_err(|_shape| crate::error::vault_data_invalid())?;
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::sweep::COMPLETE_REPAIR_VERIFICATION_CLEANUP)
            .bind(identifiers)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLEANUP))?;
        Ok(())
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
        let mut dispatched = Dispatched {
            due: due.len(),
            ..Dispatched::default()
        };

        for intent in &due {
            match self.dispatch(intent, &token, now).await {
                Ok(true) => dispatched.completed += 1,
                // The append happened and the completion did not match — this
                // pass's claim had already lapsed and another replica recorded
                // it. Counted as failed so the pacing waits, and correct
                // either way: the event exists exactly once.
                Ok(false) => dispatched.failed += 1,
                Err(failure) => {
                    dispatched.failed += 1;
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
