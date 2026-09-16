//! The read half of the outbound stream: one worker's own connection, parked
//! on `XREADGROUP BLOCK`.
//!
//! Split from [`super`] along the line the module note there draws: the queue
//! enqueues and acknowledges over the shared connection, and this reads over
//! a [`Dedicated`] one nothing else may hold. The two halves share the key,
//! the group and the field names, and nothing else.

use redis::ToRedisArgs as _;
use redis::streams::StreamReadOptions;

use super::{
    CMD_XACK, CMD_XREADGROUP, FIELD_ANSWER, FIELD_EVENT_ID, FIELD_FLEET_ID, FIELD_PROVIDER,
    FIELD_WORKSPACE_ID, NEW_ENTRIES, OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OWN_PENDING,
    OutboundDelivery,
};
use crate::dedicated::Dedicated;
use crate::error::Result;
use crate::streams::EventId;

/// The read half: one worker's own connection, which it is allowed to park on.
#[derive(Debug)]
pub struct OutboundReader {
    connection: Dedicated,
    consumer: String,
    /// Where the next resume read starts.
    ///
    /// Advances past every entry [`Self::read_pending`] hands out, and that is
    /// the whole reason it exists rather than being spelled `0` at the call.
    /// `XREADGROUP` on a pending list answers entries AFTER the id it is
    /// given, so a read fixed at `0` always answers this consumer's oldest
    /// unacknowledged entry — and an entry stays unacknowledged for as long as
    /// the lane holding it is still delivering. A caller that dispatches
    /// without waiting therefore reads the same entry back on its next turn
    /// and delivers it a second time, and a third, for as long as the first
    /// delivery takes.
    pending_cursor: String,
}

impl OutboundReader {
    /// Binds a reader to a connection nothing else holds.
    ///
    /// Takes the [`Dedicated`] by value, which is the invariant: a connection
    /// this reader will block on cannot also be somebody else's.
    #[must_use]
    pub fn new(connection: Dedicated, consumer: String) -> Self {
        Self {
            connection,
            consumer,
            pending_cursor: OWN_PENDING.to_owned(),
        }
    }

    /// The name this reader claims entries under.
    #[must_use]
    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    /// This consumer's next unacknowledged entry, without blocking.
    ///
    /// What a restart has to ask first — see the module note on pending-first.
    /// `None` means nothing is left to resume, which is the ordinary answer.
    ///
    /// NEXT, not oldest: the cursor moves past each entry handed out, so a
    /// caller walks its predecessor's work once instead of re-reading whatever
    /// is still in flight. See [`Self::pending_cursor`] for what the fixed `0`
    /// this replaced actually did.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Dragonfly is gone.
    pub async fn read_pending(&mut self) -> Result<Option<OutboundDelivery>> {
        let from = self.pending_cursor.clone();
        let delivery = self.read(&from, None).await?;
        if let Some(ref entry) = delivery {
            self.pending_cursor = entry.id.to_string();
        }
        Ok(delivery)
    }

    /// The next undelivered entry, parking up to `block_ms` for one to arrive.
    ///
    /// The park is the point: the Zig polls every 250 ms because its pooled
    /// connections could not hold a `BLOCK`, and pays that latency on every
    /// answer plus a command per interval forever. Here the server holds the
    /// read open and answers the instant an entry lands.
    ///
    /// `block_ms` bounds it anyway, because a read that never returns is a
    /// task that cannot be joined: the caller races this against its
    /// cancellation token, and dropping the future does NOT cancel the command
    /// server-side — Dragonfly may still assign an entry to this consumer after
    /// the drop. That entry is not lost, it is pending, and the next process's
    /// [`Self::read_pending`] is what finds it. Dimension 5.2.
    ///
    /// # Errors
    /// As [`Self::read_pending`].
    pub async fn read_blocking(&mut self, block_ms: usize) -> Result<Option<OutboundDelivery>> {
        self.read(NEW_ENTRIES, Some(block_ms)).await
    }

    /// One `XREADGROUP`, built the way [`crate::streams::FleetStreams`] builds
    /// its own.
    ///
    /// Through [`StreamReadOptions`] rather than by spelling `GROUP … COUNT …
    /// BLOCK …` in order: which clause `XREADGROUP` wants where is the redis
    /// crate's to know, and hand-writing it here would be a second copy of that
    /// knowledge thirty lines from the first, each free to drift.
    async fn read(
        &mut self,
        read_id: &str,
        block_ms: Option<usize>,
    ) -> Result<Option<OutboundDelivery>> {
        let mut options = StreamReadOptions::default()
            .group(OUTBOUND_CONSUMER_GROUP, &self.consumer)
            .count(1);
        if let Some(millis) = block_ms {
            options = options.block(millis);
        }

        let mut cmd = redis::cmd(CMD_XREADGROUP);
        for arg in options.to_redis_args() {
            cmd.arg(arg);
        }
        cmd.arg("STREAMS").arg(OUTBOUND_STREAM_KEY).arg(read_id);

        let reply: redis::streams::StreamReadReply = self
            .connection
            .command(CMD_XREADGROUP, OUTBOUND_STREAM_KEY, &cmd)
            .await?;
        let Some(entry) = reply.keys.into_iter().flat_map(|stream| stream.ids).next() else {
            return Ok(None);
        };

        let Some(delivery) = decode(&entry) else {
            // Dropped here rather than answered as "nothing pending", which is
            // what [`decode`]'s note has always said the sane response is — and
            // what this could not do while `None` was the only way to say it.
            //
            // The two are the same answer to a caller and opposite facts to the
            // queue. An entry that will not decode stays PENDING under this
            // consumer, so a pending-first read hands back the same entry every
            // turn, forever, and every job queued behind it waits on one row
            // nothing can deliver. One poisoned write by operator tooling or a
            // foreign writer stops outbound answers for the whole deployment.
            self.drop_undeliverable(&entry.id).await;
            return Ok(None);
        };
        Ok(Some(delivery))
    }

    /// Acknowledges an entry nothing can deliver, so the pending list drains.
    ///
    /// Logged at `warn` because it is a write this daemon did not make and
    /// cannot act on: the entry is gone after this, and the line naming its id
    /// is the only record it existed. A failed acknowledgement is not raised —
    /// the caller is mid-read on a queue that is already misbehaving, and the
    /// next turn tries again.
    async fn drop_undeliverable(&mut self, id: &str) {
        let mut cmd = redis::cmd(CMD_XACK);
        cmd.arg(OUTBOUND_STREAM_KEY)
            .arg(OUTBOUND_CONSUMER_GROUP)
            .arg(id);
        let acknowledged: Result<i64> = self
            .connection
            .command(CMD_XACK, OUTBOUND_STREAM_KEY, &cmd)
            .await;
        let event = if acknowledged.is_ok() {
            "outbound_entry_undecodable_dropped"
        } else {
            "outbound_entry_undecodable_drop_failed"
        };
        tracing::warn!(entry_id = id, event);
    }
}

/// A stream entry as a delivery, or nothing when a field is missing.
///
/// Every field is written by [`OutboundQueue::enqueue`], so an entry short of
/// one was not written by this daemon — operator tooling, a foreign writer, a
/// format that drifted. `None` rather than an error, because the caller's only
/// sane response is the same either way: acknowledge it and move on, since
/// redelivering something undeliverable forever is the one outcome worse than
/// dropping it. The Zig raises `RedisUnexpectedResponse` here and its worker
/// then swallows it, which is the same decision spelled twice.
fn decode(entry: &redis::streams::StreamId) -> Option<OutboundDelivery> {
    let field = |name: &str| entry.get::<String>(name);
    let delivery = OutboundDelivery {
        id: EventId::of(&entry.id),
        provider: field(FIELD_PROVIDER)?,
        workspace_id: field(FIELD_WORKSPACE_ID)?,
        fleet_id: field(FIELD_FLEET_ID)?,
        event_id: field(FIELD_EVENT_ID)?,
        answer: field(FIELD_ANSWER)?,
    };
    Some(delivery)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a stream entry the way Dragonfly hands one back.
    fn entry(fields: &[(&str, &str)]) -> redis::streams::StreamId {
        redis::streams::StreamId {
            id: "1700000000001-0".to_owned(),
            map: fields
                .iter()
                .map(|(name, value)| {
                    (
                        (*name).to_owned(),
                        redis::Value::BulkString((*value).as_bytes().to_vec()),
                    )
                })
                .collect(),
            // Present on a pending read and absent on a fresh one; the decoder
            // reads neither, so a plain read's shape is what is built here.
            delivered_count: None,
            milliseconds_elapsed_from_delivery: None,
        }
    }

    /// Every field an enqueue writes, which is what a complete job looks like.
    fn complete() -> Vec<(&'static str, &'static str)> {
        vec![
            (FIELD_PROVIDER, "slack"),
            (FIELD_WORKSPACE_ID, "0199a0b0-0000-7000-8000-000000000001"),
            (FIELD_FLEET_ID, "0199a0b0-0000-7000-8000-000000000002"),
            (FIELD_EVENT_ID, "1700000000000-0"),
            (FIELD_ANSWER, "Aurora is healthy."),
        ]
    }

    /// Asserted as one whole-value equality rather than field by field: a
    /// decoder that dropped a field would still pass every assertion about the
    /// fields it kept, and the failure this guards is a field going missing.
    #[test]
    fn test_decode_round_trips_every_field_and_the_entry_id() {
        let decoded = decode(&entry(&complete()));

        assert_eq!(
            decoded,
            Some(OutboundDelivery {
                id: EventId::of("1700000000001-0"),
                provider: "slack".to_owned(),
                workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
                fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
                event_id: "1700000000000-0".to_owned(),
                answer: "Aurora is healthy.".to_owned(),
            })
        );
    }

    /// One case per field, so a decoder that stopped checking one is caught by
    /// the case naming it rather than by a single entry missing everything.
    #[test]
    fn test_decode_refuses_an_entry_missing_any_required_field() {
        for (index, (name, _)) in complete().iter().enumerate() {
            let mut fields = complete();
            fields.remove(index);

            assert_eq!(
                decode(&entry(&fields)),
                None,
                "an entry with no `{name}` is not a job this daemon wrote"
            );
        }
    }

    /// The answer is model output, so it carries whatever a run produced.
    #[test]
    fn test_decode_keeps_an_answer_that_is_not_ascii() {
        let answer = "はい — 稼働中 ✅\nnewline and \"quotes\"";
        let mut fields = complete();
        fields.retain(|(name, _)| *name != FIELD_ANSWER);
        fields.push((FIELD_ANSWER, answer));

        assert_eq!(
            decode(&entry(&fields)).map(|delivered| delivered.answer),
            Some(answer.to_owned())
        );
    }
}
