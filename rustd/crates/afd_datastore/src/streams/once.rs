//! Appending an event AT MOST ONCE, however many times the caller asks.
//!
//! Two callers need that guarantee for different reasons and over different
//! retention. The repair sweep retries through crash-shaped holes until
//! Postgres records which event it produced, and forgets its key explicitly
//! once that is true. A provider redelivers a webhook on its own schedule, for
//! as long as its policy says, and nothing downstream will ever tell us the
//! delivery is settled — so that key expires on a clock instead.
//!
//! Both are the same write: claim and append, atomically, or report the id the
//! earlier call wrote. Which is why this is a Lua script rather than a command,
//! and the one write in this module that keeps a key of its own.

use super::{CMD_DEL, EventId, FleetStreams, STREAM_MAXLEN, fleet_stream_key};
use crate::error::{self, Result};

/// The command the script rides in on, named once (RULE UFS).
const CMD_EVAL: &str = "EVAL";

/// What the script says when the event was already there.
const OUTCOME_REPLAYED: &str = "replayed";

/// Where the intents that resume a fleet's own run are remembered.
const FLEET_INTENT_KEY_PREFIX: &str = "fleet:repair-verification:";

/// Where a provider delivery's idempotency slot is remembered.
///
/// `webhook_constants.zig`'s `WEBHOOK_DEDUP_KEY_PREFIX`, kept byte-for-byte: a
/// deployment mid-cutover has both daemons answering the same ingress, and two
/// spellings would let one accept a delivery the other already ran.
const WEBHOOK_DEDUP_KEY_PREFIX: &str = "webhook:dedup:";

/// How long a delivery slot outlives the delivery.
///
/// `DEDUP_TTL_SECONDS`, mirrored. A sender retrying past this window is
/// indistinguishable from a new delivery, so the window has to outlast every
/// upstream's retry schedule — a day covers GitHub, Slack and Svix with room.
const WEBHOOK_DEDUP_TTL_SECONDS: u64 = 86_400;

/// How long an App delivery's slot outlives it.
///
/// `github.zig:42` shadows the shared `DEDUP_TTL_SECONDS` with this value and
/// pins it with a test named for what it covers: *"replay slot covers the
/// GitHub redelivery window"*. Three days rather than one because the window
/// this has to outlast is not the AUTOMATIC retry schedule a day already
/// covers — it is an operator opening a provider's delivery log and pressing
/// Redeliver, which GitHub allows for three days. A claim that expired first
/// would let that button run the fleet a second time on the same event.
///
/// The per-fleet path deliberately keeps the shorter window: a delivery there
/// is addressed to one fleet by a URL its owner configured, and widening a
/// live claim window is a behaviour change this milestone did not ask for.
const APP_DEDUP_TTL_SECONDS: u64 = 3 * WEBHOOK_DEDUP_TTL_SECONDS;

/// The key prefix a schedule fire's claim lives under.
///
/// Its own namespace rather than the webhook one, because the two are keyed on
/// different things — a webhook claim is per sender event id, a fire claim is
/// per schedule and scheduler message id — and one prefix over two key shapes
/// is a collision waiting for the day the shapes converge.
const SCHEDULE_FIRE_KEY_PREFIX: &str = "schedule:fire:";

/// How long a fire's claim outlives the fire.
///
/// The external scheduler retries a callback it did not get a 2xx for, and
/// gives up well inside a day. A window equal to the webhook one is therefore
/// already generous, and matching it means one number to reason about rather
/// than two that drifted.
const SCHEDULE_FIRE_TTL_SECONDS: u64 = WEBHOOK_DEDUP_TTL_SECONDS;

/// No expiry — the key is forgotten by name, not by clock.
const NO_EXPIRY: u64 = 0;

/// What an at-most-once claim is remembered for.
///
/// A closed enum rather than a caller-supplied prefix and window. The two
/// fields travel together — a namespace with the wrong retention is a slot that
/// either expires while its sender is still retrying or never expires at all —
/// and a signature taking them apart lets a call site pair them wrongly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnceScope {
    /// An intent that resumes a fleet's own run: a repair verification, and
    /// the continuation an approved gate lands.
    ///
    /// One namespace for both, because both answer the same question — does
    /// this fleet already carry the event this intent produces. The spelling is
    /// the repair path's because it claimed the prefix first, and renaming it
    /// would orphan every in-flight key at the deploy that renamed it. Named
    /// here rather than left to be rediscovered from the string.
    FleetIntent,
    /// A provider's delivery, deduplicated per fleet and sender event id.
    ///
    /// The id a caller passes is `{fleet_id}:{event_id}` — composed by the
    /// ingress handler, because which field of which envelope is the sender's
    /// idempotency key is the envelope's contract, not this module's.
    WebhookDelivery,
    /// A provider App's delivery, deduplicated per fleet and body digest.
    ///
    /// The same key prefix and the same per-fleet composition as
    /// [`Self::WebhookDelivery`] — one App delivery fans out to many fleets and
    /// each one claims separately — differing only in how long the claim lives.
    /// See [`APP_DEDUP_TTL_SECONDS`].
    AppDelivery,
    /// A schedule fire, deduplicated per fleet, schedule and message id.
    ///
    /// The id a caller passes is `{fleet_id}:{schedule_id}:{message_id}` — the
    /// schedule is in the key because one fleet holds many, and two of them
    /// firing on the same tick must not silence each other.
    ScheduleFire,
}

impl OnceScope {
    /// The key prefix this scope's claims live under.
    const fn prefix(self) -> &'static str {
        match self {
            Self::FleetIntent => FLEET_INTENT_KEY_PREFIX,
            Self::WebhookDelivery | Self::AppDelivery => WEBHOOK_DEDUP_KEY_PREFIX,
            Self::ScheduleFire => SCHEDULE_FIRE_KEY_PREFIX,
        }
    }

    /// How many seconds a claim survives, or [`NO_EXPIRY`] for forever.
    const fn ttl_seconds(self) -> u64 {
        match self {
            // Forgotten by `forget_once` after the database records what the
            // intent produced. A clock here would let a retry through that
            // window append a second event.
            Self::FleetIntent => NO_EXPIRY,
            Self::WebhookDelivery => WEBHOOK_DEDUP_TTL_SECONDS,
            Self::ScheduleFire => SCHEDULE_FIRE_TTL_SECONDS,
            Self::AppDelivery => APP_DEDUP_TTL_SECONDS,
        }
    }

    /// The key one claim is remembered under.
    ///
    /// One spelling (RULE UFS): [`FleetStreams::append_once`] writes it and
    /// [`FleetStreams::forget_once`] deletes it, and a pair that drifted would
    /// leave the write remembered forever while the delete removed nothing — an
    /// intent that could never run again.
    fn key(self, once_id: &str) -> String {
        format!("{}{once_id}", self.prefix())
    }
}

/// Appends one event and remembers that it did, atomically.
///
/// Lua because the two operations must not be separable: a `SET NX` and an
/// `XADD` as two round trips leave a crash-shaped hole between them, and the
/// caller retrying through that hole is exactly the case this exists for. The
/// Zig ingress carries the two-round-trip version and pays for it with a
/// release-the-slot arm on every post-claim failure path; there is no such arm
/// here, because there is no window for one to cover.
///
/// The type check is not defensive padding — a key that is not a stream means
/// something else in this deployment is using the name, and appending to it
/// would corrupt whatever that is.
///
/// `ARGV[1]` is the trim length, `ARGV[2]` the expiry in seconds (`0` for
/// none), and the field pairs follow from `ARGV[3]`.
static APPEND_ONCE: std::sync::LazyLock<redis::Script> = std::sync::LazyLock::new(|| {
    redis::Script::new(
        r"local existing = redis.call('GET', KEYS[1])
if existing then return {existing, 'replayed'} end
local kind = redis.call('TYPE', KEYS[2]).ok
if kind ~= 'none' and kind ~= 'stream' then
  return redis.error_reply('append-once target is not a stream')
end
local event_id = redis.call('XADD', KEYS[2], 'MAXLEN', '~', ARGV[1], '*', unpack(ARGV, 3))
local ttl = tonumber(ARGV[2])
if ttl > 0 then
  redis.call('SET', KEYS[1], event_id, 'EX', ttl)
else
  redis.call('SET', KEYS[1], event_id)
end
return {event_id, 'emitted'}",
    )
});

/// What an append-once call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appended {
    /// The event's identifier — this call's, or the earlier call's.
    pub id: EventId,
    /// Whether an earlier call already wrote it.
    pub replayed: bool,
}

impl FleetStreams {
    /// Appends one event AT MOST ONCE, however many times this is called.
    ///
    /// The second attempt returns the FIRST attempt's event id rather than
    /// appending a second event. For a repair verification a duplicate would
    /// run the same verification twice with real provider spend; for a webhook
    /// it would run the fleet twice on one delivery. Same guarantee, same
    /// script, different retention — see [`OnceScope`].
    ///
    /// Answers the event id and whether this call is the one that wrote it.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// key holding something that is not a stream is refused by the script
    /// rather than appended to.
    pub async fn append_once(
        &self,
        scope: OnceScope,
        once_id: &str,
        fleet_id: &str,
        fields: &[(&str, &str)],
    ) -> Result<Appended> {
        let key = fleet_stream_key(fleet_id);
        let mut invocation = APPEND_ONCE.prepare_invoke();
        invocation
            .key(scope.key(once_id))
            .key(&key)
            .arg(STREAM_MAXLEN)
            .arg(scope.ttl_seconds());
        for (name, value) in fields {
            invocation.arg(*name).arg(*value);
        }

        let (event_id, outcome): (String, String) =
            self.redis.script(CMD_EVAL, &key, &invocation).await?;
        if event_id.is_empty() {
            return Err(error::unexpected_reply(CMD_EVAL));
        }
        Ok(Appended {
            id: EventId(event_id),
            replayed: outcome == OUTCOME_REPLAYED,
        })
    }

    /// Forgets an append-once key.
    ///
    /// Called only AFTER the database records which event the intent produced.
    /// Cleared any earlier and a retry in between would append a second event,
    /// which is the exact duplicate the key exists to prevent. A scope whose
    /// claims expire on their own never needs this.
    ///
    /// # Errors
    /// Returns a command error when the delete fails.
    pub async fn forget_once(&self, scope: OnceScope, once_id: &str) -> Result<()> {
        let key = scope.key(once_id);
        let mut cmd = redis::cmd(CMD_DEL);
        cmd.arg(&key);
        let _removed: i64 = self.redis.command(CMD_DEL, &key, &cmd).await?;
        Ok(())
    }
}
