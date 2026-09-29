//! The exclusive hub lane's shared moves: find the nodes, name the hub's own
//! connections on each, kill them by id, and wait for delivery to resume.
//!
//! Dragonfly answers `CLIENT KILL TYPE pubsub` with a syntax error and its
//! `CLIENT LIST` carries no subscriber marker, so the only handle on "the
//! hub's connection" is a diff of every node's client set around the hub's
//! start. Every module that uses these must run alone; see
//! `make/test-integration-rustd.mk`'s exclusive filter.

use std::collections::BTreeSet;
use std::time::Duration;

use afd_dragonfly::hub::Received;
use afd_dragonfly::streams::FleetStreams;

use crate::support::DragonflyHarness;

/// How long a redial, a re-subscribe or a delivery is given.
pub(crate) const RECOVERY_BUDGET: Duration = Duration::from_secs(10);

/// How often the conditions above are re-read while waiting.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// One node's address, as `CLUSTER SLOTS` advertises it.
pub(crate) type Node = String;

/// Every node address the cluster advertises, primaries and replicas.
///
/// Read from `CLUSTER SLOTS` rather than assumed from the seed: the lane's
/// ports are the compose file's business, and a test that hard-coded them
/// would pass against the wrong cluster.
pub(crate) async fn nodes_of(harness: &DragonflyHarness) -> BTreeSet<Node> {
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

/// The node serving `slot`: the first address of the `CLUSTER SLOTS` range
/// that holds it, which is that range's primary.
pub(crate) async fn owner_of(harness: &DragonflyHarness, slot: u16) -> Node {
    let mut cmd = redis::cmd("CLUSTER");
    cmd.arg("SLOTS");
    let reply: redis::Value = harness
        .redis
        .command("CLUSTER", "slots", &cmd)
        .await
        .expect("CLUSTER SLOTS answers");
    let redis::Value::Array(ranges) = reply else {
        panic!("CLUSTER SLOTS is an array of ranges: {reply:?}");
    };
    ranges
        .iter()
        .find_map(|range| {
            let redis::Value::Array(fields) = range else {
                return None;
            };
            let [
                redis::Value::Int(start),
                redis::Value::Int(end),
                primary,
                ..,
            ] = fields.as_slice()
            else {
                return None;
            };
            let mut owner = std::collections::BTreeSet::new();
            collect_nodes(primary, &mut owner);
            (*start <= i64::from(slot) && i64::from(slot) <= *end)
                .then(|| owner.into_iter().next())
                .flatten()
        })
        .unwrap_or_else(|| panic!("no range serves slot {slot}"))
}

/// The client ids each node currently holds.
///
/// A direct, non-cluster connection per node: `CLIENT LIST` names no key, so a
/// cluster client would route it to a node of the driver's choosing and two
/// calls could answer about two different servers.
pub(crate) async fn clients_across(nodes: &BTreeSet<Node>) -> BTreeSet<(Node, i64)> {
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
pub(crate) async fn kill_each(nodes: &BTreeSet<Node>, victims: &BTreeSet<(Node, i64)>) {
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
    let config = DragonflyHarness::config();
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
pub(crate) fn difference(
    now: &BTreeSet<(Node, i64)>,
    before: &BTreeSet<(Node, i64)>,
) -> BTreeSet<(Node, i64)> {
    now.difference(before).cloned().collect()
}

/// Publishes `payload` until the reader sees it, and asserts it arrives once.
pub(crate) async fn deliver(
    publisher: &FleetStreams,
    channel: &str,
    payload: &str,
    reader: &mut afd_dragonfly::Subscription,
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
            // A lag or gap notice, or an empty poll: publish again and keep
            // waiting.
            Ok(Ok(Received::Lagged(..) | Received::Gap)) | Err(..) => {}
            Ok(Err(closed)) => panic!("the reader's channel closed: {closed}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{payload} never reached the reader within {RECOVERY_BUDGET:?}"
        );
    }
}

/// Waits until `reader` is told its subscription was lost and is back.
pub(crate) async fn gap_on(reader: &mut afd_dragonfly::Subscription) {
    let told = tokio::time::timeout(RECOVERY_BUDGET, async {
        loop {
            match reader
                .recv()
                .await
                .expect("the reader's channel stays open")
            {
                Received::Gap => return,
                Received::Message(_) | Received::Lagged(_) => {}
            }
        }
    })
    .await;
    assert!(
        told.is_ok(),
        "{} was never told about its gap",
        reader.channel()
    );
}

/// How many subscribers the server counts on `channel`.
pub(crate) async fn subscribers_on(harness: &DragonflyHarness, channel: &str) -> i64 {
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
pub(crate) async fn wait_for<F, Fut>(what: &str, mut condition: F)
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
