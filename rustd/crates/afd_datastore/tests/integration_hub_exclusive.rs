//! The hub recovers from a connection the SERVER tore down.
//!
//! `integration_hub` proves the recovery logic from inside: given that the
//! pump's connection ends, the hub redials and re-subscribes. This file proves
//! the half that logic cannot reach, and the half a datastore migration is
//! actually risking:
//!
//! ```text
//!   Dragonfly closes the socket
//!        -> TCP/TLS surfaces EOF or reset
//!             -> the redis-rs cluster driver ends push delivery
//!                  -> the pump notices
//!                       -> the hub redials and re-subscribes
//! ```
//!
//! A test that ends the pump from inside this process starts at the fourth
//! arrow and asserts the fifth. Every arrow above it is the compatibility
//! surface being migrated, so skipping them turns a known gap into an untested
//! guarantee.
//!
//! # Why this file runs alone
//!
//! Dragonfly answers `CLIENT KILL TYPE pubsub` with a syntax error -- it
//! implements `ADDR`, `LADDR` and `ID` and nothing narrower -- and its
//! `CLIENT LIST` carries no `sub=`/`psub=` marker, defaulting a connection's
//! name to its own id. Measured against `dragonfly_version:df-v1.40.2`, which
//! reports `redis_version:7.4.0` while implementing neither. So there is no
//! server-side handle for "the hub's connection", and the only way to find it
//! is to snapshot every node's clients, start the hub, and diff.
//!
//! That diff is only sound while nothing else opens a connection, which is why
//! this module is filtered out of the parallel lane and run again on its own
//! (`make/test-integration-rustd.mk`). It is a hard gate either way: the lane's
//! own guard fails a selection that matches nothing.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_datastore::SubscriptionHub;
use afd_datastore::hub::Received;
use afd_datastore::streams::FleetStreams;
use backon::ExponentialBuilder;

use crate::support::RedisHarness;

/// How long a redial, a re-subscribe or a delivery is given.
const RECOVERY_BUDGET: Duration = Duration::from_secs(10);

/// How often the conditions above are re-read while waiting.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Payload published before the kill, to prove the reader was live first.
const BEFORE: &str = "before-the-kill";

/// Payload published after it. Distinct from [`BEFORE`] so a stale frame
/// cannot be mistaken for recovery.
const AFTER: &str = "after-the-kill";

/// One node's address, as `CLUSTER SLOTS` advertises it.
type Node = String;

/// The hub survives the server killing its connection, and keeps delivering.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_server_killed_connection_is_redialled_and_its_channels_resubscribed() {
    let harness = RedisHarness::connect().await;
    let publisher = FleetStreams::new(harness.redis.clone());
    let channel = harness.name("exclusive-channel");

    // Every connection this test owns is opened BEFORE the first snapshot, so
    // it appears in both and never lands in the diff.
    let nodes = nodes_of(&harness).await;
    assert!(
        !nodes.is_empty(),
        "the cluster advertises at least one node"
    );
    let before_hub = clients_across(&nodes).await;

    let hub = SubscriptionHub::start_with_backoff(
        RedisHarness::config(),
        ExponentialBuilder::new()
            .with_min_delay(Duration::from_millis(20))
            .with_max_delay(Duration::from_millis(100)),
    )
    .await
    .expect("hub starts");
    let mut reader = hub.subscribe(&channel);
    deliver(&publisher, &channel, BEFORE, &mut reader).await;

    let generation = hub.connections_opened();
    let opened = difference(&clients_across(&nodes).await, &before_hub);
    assert!(
        !opened.is_empty(),
        "the hub opened no connection this test can name, so the kill below \
         would prove nothing -- snapshots taken while another test was \
         connecting is the one way this goes wrong"
    );

    kill_each(&nodes, &opened).await;

    // Generation, not socket count: `record_connection` fires once at spawn
    // and once per redial (`hub/pump.rs`), so this counts times the hub has
    // had a connection, which is the thing recovery advances.
    wait_for("the hub redials", || async {
        hub.connections_opened() > generation
    })
    .await;
    // Not "went to zero first": a fast redial can make zero unobservable, and
    // a test that demanded it would fail on a healthy system that recovered
    // too quickly. One subscriber at the end is the claim.
    wait_for("the channel is re-subscribed", || async {
        subscribers_on(&harness, &channel).await == 1
    })
    .await;

    // The reader is the SAME one, held across the kill. Re-subscribing a
    // channel nobody is left listening to would satisfy every count above and
    // deliver nothing.
    deliver(&publisher, &channel, AFTER, &mut reader).await;
}

/// Every node address the cluster advertises, primaries and replicas.
///
/// Read from `CLUSTER SLOTS` rather than assumed from the seed: the lane's
/// ports are the compose file's business, and a test that hard-coded them
/// would pass against the wrong cluster.
async fn nodes_of(harness: &RedisHarness) -> BTreeSet<Node> {
    let mut cmd = redis::cmd("CLUSTER");
    cmd.arg("SLOTS");
    let reply: redis::Value = harness
        .redis
        .command("CLUSTER", "slots", &cmd)
        .await
        .expect("CLUSTER SLOTS answers");
    let mut nodes = BTreeSet::new();
    collect_nodes(&reply, &mut nodes);
    nodes
}

/// Walks a `CLUSTER SLOTS` reply for every `[host, port, ...]` triple.
///
/// Recursive over the nesting rather than indexed into it: the reply is
/// `[start, end, [host, port, id], ...]` per range, and a server that adds a
/// field would break positional reads while leaving this one correct.
fn collect_nodes(value: &redis::Value, out: &mut BTreeSet<Node>) {
    let redis::Value::Array(items) = value else {
        return;
    };
    if let [redis::Value::BulkString(host), redis::Value::Int(port), ..] = items.as_slice()
        && let Ok(host) = std::str::from_utf8(host)
    {
        out.insert(format!("{host}:{port}"));
    }
    for item in items {
        collect_nodes(item, out);
    }
}

/// The client ids each node currently holds.
///
/// A direct, non-cluster connection per node: `CLIENT LIST` names no key, so a
/// cluster client would route it to a node of the driver's choosing and two
/// calls could answer about two different servers.
async fn clients_across(nodes: &BTreeSet<Node>) -> BTreeSet<(Node, i64)> {
    let mut seen = BTreeSet::new();
    for node in nodes {
        for id in client_ids(node).await {
            seen.insert((node.clone(), id));
        }
    }
    seen
}

/// `CLIENT LIST` against one node, reduced to its `id=` fields.
async fn client_ids(node: &Node) -> Vec<i64> {
    let mut connection = node_connection(node).await;
    let listing: String = redis::cmd("CLIENT")
        .arg("LIST")
        .query_async(&mut connection)
        .await
        .expect("CLIENT LIST answers");
    listing
        .lines()
        .filter_map(|row| row.split_whitespace().next())
        .filter_map(|field| field.strip_prefix("id="))
        .filter_map(|id| id.parse::<i64>().ok())
        .collect()
}

/// Kills each id on the node that holds it.
///
/// `CLIENT KILL ID` because Dragonfly implements no narrower selector; see the
/// module header. A zero reply is not an error -- the connection may already
/// have gone -- so the assertion that matters is the recovery below, not this.
async fn kill_each(nodes: &BTreeSet<Node>, victims: &BTreeSet<(Node, i64)>) {
    for (node, id) in victims {
        debug_assert!(nodes.contains(node));
        let mut connection = node_connection(node).await;
        let _killed: i64 = redis::cmd("CLIENT")
            .arg("KILL")
            .arg("ID")
            .arg(*id)
            .query_async(&mut connection)
            .await
            .expect("CLIENT KILL ID answers");
    }
}

/// A plain connection to one node, authenticated the way the lane's seed is.
async fn node_connection(node: &Node) -> redis::aio::MultiplexedConnection {
    let config = RedisHarness::config();
    let seed = config.url();
    let credentials = seed
        .split_once("//")
        .and_then(|(_scheme, rest)| rest.split_once('@'))
        .map_or(String::new(), |(auth, _host)| format!("{auth}@"));
    let url = format!("redis://{credentials}{node}");
    redis::Client::open(url.as_str())
        .expect("the node url is well formed")
        .get_multiplexed_async_connection()
        .await
        .expect("the node answers a direct connection")
}

/// What the members of `now` are that `before` did not hold.
fn difference(
    now: &BTreeSet<(Node, i64)>,
    before: &BTreeSet<(Node, i64)>,
) -> BTreeSet<(Node, i64)> {
    now.difference(before).cloned().collect()
}

/// Publishes `payload` until the reader sees it, and asserts it arrives once.
async fn deliver(
    publisher: &FleetStreams,
    channel: &str,
    payload: &str,
    reader: &mut afd_datastore::Subscription,
) {
    let deadline = tokio::time::Instant::now() + RECOVERY_BUDGET;
    loop {
        publisher
            .publish(channel, payload)
            .await
            .expect("the publish reaches the datastore");
        match tokio::time::timeout(POLL_INTERVAL, reader.recv()).await {
            Ok(Ok(Received::Message(message))) => {
                assert_eq!(message.payload, payload, "the reader saw a stale frame");
                return;
            }
            // A lag notice or an empty poll: publish again and keep waiting.
            Ok(Ok(Received::Lagged(..))) | Err(..) => {}
            Ok(Err(closed)) => panic!("the reader's channel closed: {closed}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{payload} never reached the reader within {RECOVERY_BUDGET:?}"
        );
    }
}

/// How many subscribers the server counts on `channel`.
async fn subscribers_on(harness: &RedisHarness, channel: &str) -> i64 {
    let mut cmd = redis::cmd("PUBSUB");
    cmd.arg("NUMSUB").arg(channel);
    let reply: Vec<redis::Value> = harness
        .redis
        .command("PUBSUB", channel, &cmd)
        .await
        .expect("PUBSUB NUMSUB");
    // RESP3 answers a MAP of channel -> count; the flat RESP2 pair this would
    // otherwise index at 1 renders identically under `redis-cli`.
    match reply.as_slice() {
        [redis::Value::Map(entries)] => match entries.as_slice() {
            [(_, redis::Value::Int(count))] => *count,
            other => panic!("NUMSUB names one channel, got: {other:?}"),
        },
        other => panic!("unexpected NUMSUB reply: {other:?}"),
    }
}

/// Waits for `condition`, naming it if the budget runs out.
async fn wait_for<F, Fut>(what: &str, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + RECOVERY_BUDGET;
    while !condition().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{what} did not happen within {RECOVERY_BUDGET:?}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
