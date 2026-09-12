//! The lane's Dragonfly cluster: a seed to dial, a way to move its slots, and
//! keys nothing else in the suite will touch.
//!
//! Distinct from [`crate::support::RedisHarness`] in the one way that matters:
//! this hands out the DRIVER's cluster connection rather than the crate's
//! `Redis`, because the prototypes it serves exist to learn what the driver
//! and the server do before the boundary is built over them. Once a boundary
//! suite exists for a behaviour, the prototype that discovered it retires.
//!
//! # Moving slots is a lane-wide act
//!
//! A migration is pushed to every node as one configuration document, and two
//! migrations pushed concurrently overwrite each other. Every test that moves
//! a slot holds [`CLUSTER_LANE`] for its whole body and moves the slot back
//! before releasing it, so the next test finds the canonical layout — the
//! integration lane's reset restores it as well, for the test that failed
//! halfway.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use redis::cluster::ClusterClientBuilder;
use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};
use redis::{FromRedisValue as _, ProtocolVersion, PushInfo, Value};
use tokio::sync::{Mutex, mpsc};

use crate::subscriber::install_subscriber;

/// The knob `make test-integration-rustd` exports the cluster's seed under.
const URL_KNOB: &str = "TEST_DRAGONFLY_URL";

/// The knob carrying the command line that moves slots and resets the
/// cluster, run inside the cluster's own container.
const CONTROL_KNOB: &str = "TEST_DRAGONFLY_CONTROL";

/// The control script's verb for moving one slot range.
const CONTROL_MIGRATE: &str = "migrate";

/// `CLUSTER KEYSLOT`, the server's own answer to which slot a key hashes to.
const CMD_CLUSTER: &str = "CLUSTER";
const SUB_KEYSLOT: &str = "KEYSLOT";

/// The first slot the second primary owns in the canonical layout — the same
/// number `scripts/dragonfly-cluster.sh` spells in `CANONICAL_SLOTS`.
const CANONICAL_SPLIT: u16 = 8192;

/// The two primaries, by the index the control script knows them under.
pub(crate) const PRIMARY_A: u8 = 0;
pub(crate) const PRIMARY_B: u8 = 2;

/// How long one reply may take, and how long a dial may.
const RESPONSE_BUDGET: Duration = Duration::from_secs(5);
const CONNECT_BUDGET: Duration = Duration::from_secs(5);

/// How many times the driver follows a redirect before giving up. A migration
/// answers `MOVED` to whoever asks mid-flight, and each follow costs a slot
/// refresh; eight covers a move that lands while a refresh is in progress.
const REDIRECT_RETRIES: u32 = 8;

/// Distinguishes keys minted by one process.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Serialises every test that moves a slot. See the module documentation.
pub(crate) static CLUSTER_LANE: Mutex<()> = Mutex::const_new(());

/// The lane's cluster, plus a name nothing else in the suite uses.
pub(crate) struct ClusterHarness {
    seed: String,
    control: String,
    prefix: String,
}

impl ClusterHarness {
    /// Reads the lane's knobs. Panics outside the lane, where every assertion
    /// after this point would be about a cluster that does not exist.
    pub(crate) fn from_lane() -> Self {
        install_subscriber();
        let seed = std::env::var(URL_KNOB).unwrap_or_else(|_unset| {
            panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        let control = std::env::var(CONTROL_KNOB).unwrap_or_else(|_unset| {
            panic!("{CONTROL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        Self {
            seed,
            control,
            prefix: format!(
                "afdc{}_{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    /// A name unique to this harness, so parallel tests never collide.
    pub(crate) fn name(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }

    /// The driver's builder over the lane's seed: RESP3, so pushes can be
    /// delivered, bounded dials and replies, and a redirect allowance.
    fn builder(&self) -> ClusterClientBuilder {
        ClusterClientBuilder::new([self.seed.clone()])
            .use_protocol(ProtocolVersion::RESP3)
            .connection_timeout(CONNECT_BUDGET)
            .response_timeout(RESPONSE_BUDGET)
            .retries(REDIRECT_RETRIES)
    }

    /// A connection for ordinary commands.
    pub(crate) async fn connect(&self) -> ClusterConnection {
        self.builder()
            .build()
            .expect("the seed URL parses")
            .get_async_connection()
            .await
            .expect("the lane's cluster must be reachable")
    }

    /// A connection whose pushes — subscription confirmations, messages,
    /// disconnections — arrive on the returned receiver.
    pub(crate) async fn connect_with_pushes(
        &self,
    ) -> (ClusterConnection, mpsc::UnboundedReceiver<PushInfo>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let connection = self
            .builder()
            .push_sender(sender)
            .build()
            .expect("the seed URL parses")
            .get_async_connection()
            .await
            .expect("the lane's cluster must be reachable");
        (connection, receiver)
    }

    /// A connection one caller owns alone, allowed to park for `longest_park`
    /// before a reply is given up on — the cluster shape of
    /// `afd_redis::Dedicated`.
    pub(crate) async fn connect_parked(&self, longest_park: Duration) -> ClusterConnection {
        self.builder()
            .response_timeout(longest_park + RESPONSE_BUDGET)
            .build()
            .expect("the seed URL parses")
            .get_async_connection()
            .await
            .expect("the lane's cluster must be reachable")
    }

    /// The slot `key` hashes to, as the server computes it.
    pub(crate) async fn keyslot(connection: &mut ClusterConnection, key: &str) -> u16 {
        let mut cmd = redis::cmd(CMD_CLUSTER);
        cmd.arg(SUB_KEYSLOT).arg(key);
        let reply = connection
            .route_command(cmd, RoutingInfo::SingleNode(SingleNodeRoutingInfo::Random))
            .await
            .expect("CLUSTER KEYSLOT answers on any node");
        u16::from_redis_value(reply).expect("a slot is a small integer")
    }

    /// Which primary owns `slot` in the canonical layout.
    pub(crate) const fn canonical_primary(slot: u16) -> u8 {
        if slot < CANONICAL_SPLIT {
            PRIMARY_A
        } else {
            PRIMARY_B
        }
    }

    /// The primary that is not `primary`.
    pub(crate) const fn other_primary(primary: u8) -> u8 {
        if primary == PRIMARY_A {
            PRIMARY_B
        } else {
            PRIMARY_A
        }
    }

    /// Moves one slot between primaries, blocking until the server reports the
    /// migration finished and the successor configuration is pushed.
    pub(crate) async fn move_slot(&self, slot: u16, from: u8, to: u8) {
        self.control(&format!("{CONTROL_MIGRATE} {from} {to} {slot} {slot}"))
            .await;
    }

    /// Runs the control script with `args` appended, off the runtime's
    /// threads: it is a docker round trip, and a blocking wait on a reactor
    /// thread would stall every other test in the binary.
    async fn control(&self, args: &str) {
        let command_line = format!("{} {args}", self.control);
        let output = tokio::task::spawn_blocking(move || {
            Command::new("sh").arg("-c").arg(&command_line).output()
        })
        .await
        .expect("the control task is not cancelled")
        .expect("sh is on every lane host");
        assert!(
            output.status.success(),
            "cluster control `{args}` failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// The text of a push's `index`th datum, for the message and channel fields.
pub(crate) fn push_text(push: &PushInfo, index: usize) -> Option<String> {
    push.data.get(index).and_then(|value| match value {
        Value::BulkString(bytes) => String::from_utf8(bytes.clone()).ok(),
        Value::SimpleString(text) => Some(text.clone()),
        _other => None,
    })
}
