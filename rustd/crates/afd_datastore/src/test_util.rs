//! Bounded live-Redis connection setup for datastore-backed tests.
//!
//! A Rust test binary runs its tests in parallel. Opening one TLS connection
//! per test makes connection setup the bottleneck and can exhaust a short boot
//! deadline before Redis has accepted every handshake. Each `#[tokio::test]`
//! also owns a distinct runtime, so its connection cannot outlive that runtime
//! and be shared process-wide. Serializing just the handshake keeps
//! every manager on its owning runtime without flooding the TLS listener.

use std::time::Duration;

use tokio::sync::Semaphore;

use crate::Redis;
use crate::config::RedisConfig;
use crate::error::{ErrorKind, Result};
use crate::streams::{EventId, FleetEvent};

static CONNECT_SERIAL: Semaphore = Semaphore::const_new(1);

/// How many times a lane connection may lose the CPU race before it is a fault.
///
/// Measured on the compose listener: plain TCP to the same port answers in
/// 0.1 ms at the median and never exceeded 6.2 ms across 150 samples, so the
/// transport is not what costs. The whole 246 ms median — and its multi-second
/// tail — is the rustls handshake against an RSA-2048 certificate, redone for
/// every connection with no session resumption. That is CPU work, and it
/// competes with the suite that asked for it: under a loaded machine the
/// handshake queues behind compilation and other tests until it passes the
/// connect budget.
///
/// So a lapsed budget here means "the machine was busy", not "Redis is down",
/// and three attempts distinguish them. A genuinely absent Redis fails three
/// times quickly and still fails; a contended one wins a later attempt.
const CONNECT_RETRY_ATTEMPTS: u32 = 3;

/// How long to wait after a lapsed attempt, giving the CPU spike time to pass.
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// Opens a live connection without competing with another test's handshake.
///
/// Fault-injection tests with private endpoints should keep using
/// [`Redis::connect`] or [`Redis::unreachable`] directly: only concurrent
/// connections to the lane's one TLS listener need this admission gate.
///
/// # Errors
/// Returns the connection attempt's configuration or transport failure.
pub async fn connect_live(config: &RedisConfig) -> Result<Redis> {
    let mut attempt = 1;
    loop {
        // This private semaphore is never closed. Keeping the acquisition
        // result alive holds the permit for the handshake; `Err` is
        // unconstructible while the only code with access to the semaphore
        // never calls `close`. It is re-acquired per attempt so a retry queues
        // behind other tests rather than holding the listener for its backoff.
        let outcome = {
            let _permit = CONNECT_SERIAL.acquire().await;
            Redis::connect(config).await
        };
        match outcome {
            Ok(redis) => return Ok(redis),
            // Only a lapsed deadline is retried. A refused endpoint, an
            // unreadable certificate authority, or a malformed URL will answer
            // the same way three times, and retrying them turns a one-second
            // diagnosis into a three-second one that reports the same fault.
            Err(error)
                if matches!(error.kind(), ErrorKind::ConnectTimeout { .. })
                    && attempt < CONNECT_RETRY_ATTEMPTS =>
            {
                attempt += 1;
                tokio::time::sleep(RETRY_BACKOFF).await;
            }
            Err(error) => return Err(error),
        }
    }
}

/// The synchronous half of a connect, for the timing diagnostic.
///
/// The transport reads the certificate authority off disk and builds the TLS
/// client INLINE — no `spawn_blocking` — so its cost is paid on whichever
/// worker polls the connect. This seam lets a test measure it.
///
/// # Errors
/// Returns a config error when the seed is not a URL or a named authority is
/// unreadable.
pub fn build_client_for_diagnosis(config: &RedisConfig) -> Result<redis::cluster::ClusterClient> {
    crate::transport::client(config, config.request_timeout())
}

/// The `CLUSTER SLOTS` reply a fake server must answer before a cluster client
/// will send it anything else.
///
/// One shard owning all 16384 slots at `port`, with an EMPTY hostname — which
/// is what tells the driver to keep dialling the address the connection came in
/// on rather than resolving a name the fake does not have.
///
/// Shared rather than respelled per fake. There are two in this workspace and
/// the handshake was added to only one of them when the transport became
/// cluster-only, which left the other answering `+PONG` to a client that never
/// got far enough to ping — a failure whose message (`ClusterConnectionNotFound`)
/// names nothing a reader would connect to a missing handshake.
#[must_use]
pub fn cluster_slots_reply(port: u16) -> Vec<u8> {
    format!("*1\r\n*3\r\n:0\r\n:16383\r\n*2\r\n$0\r\n\r\n:{port}\r\n").into_bytes()
}

/// The `CLUSTER SHARDS` twin of [`cluster_slots_reply`].
///
/// One shard, one primary at `127.0.0.1:port`, in the flat pair framing
/// Dragonfly answers with — which is the shape `topology` parses, and the
/// one every per-node walk (a scan, an `INFO`) needs before it can route by
/// address.
#[must_use]
pub fn cluster_shards_reply(port: u16) -> Vec<u8> {
    format!(
        "*1\r\n*4\r\n$5\r\nslots\r\n*2\r\n:0\r\n:16383\r\n$5\r\nnodes\r\n*1\r\n\
         *8\r\n$2\r\nip\r\n$9\r\n127.0.0.1\r\n$4\r\nport\r\n:{port}\r\n\
         $4\r\nrole\r\n$6\r\nmaster\r\n$6\r\nhealth\r\n$6\r\nonline\r\n"
    )
    .into_bytes()
}

/// Every entry one fleet's stream holds, oldest first.
///
/// `XRANGE` over the whole stream, NOT a group read: `XREADGROUP` moves
/// `last-delivered-id`, which moves the retention floor, so a suite that
/// inspected its stream that way would change the thing it was about to assert
/// on. Nothing in production reads a stream this way — history is
/// `core.fleet_events` — which is why it lives here and not on
/// [`FleetStreams`](crate::FleetStreams).
///
/// Each [`FleetEvent`] carries the receipt and the entry's fields, so a caller
/// reads the logical id with `event.field(afd_wire::event::field::EVENT_ID)`
/// rather than spelling the field name here. After a replay one logical id
/// legitimately appears on two entries, and both are returned.
///
/// # Errors
/// Returns a command error when the stream cannot be read.
pub async fn fleet_entries(redis: &Redis, fleet_id: &str) -> Result<Vec<FleetEvent>> {
    let key = crate::streams::fleet_stream_key(fleet_id);
    let mut cmd = redis::cmd(CMD_XRANGE);
    cmd.arg(&key).arg(RANGE_OLDEST).arg(RANGE_NEWEST);
    let reply: redis::streams::StreamRangeReply = redis.command(CMD_XRANGE, &key, &cmd).await?;
    Ok(reply
        .ids
        .iter()
        .map(|entry| FleetEvent {
            receipt: EventId::of(&entry.id),
            fields: entry
                .map
                .iter()
                .map(|(name, value)| (name.clone(), crate::streams::render::stringify(value)))
                .collect(),
        })
        .collect())
}

/// The command [`fleet_entries`] issues, and its bounds.
const CMD_XRANGE: &str = "XRANGE";
const RANGE_OLDEST: &str = "-";
const RANGE_NEWEST: &str = "+";
