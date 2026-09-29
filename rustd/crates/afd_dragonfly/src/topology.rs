//! What the cluster looks like right now, asked of the cluster itself.
//!
//! The driver keeps a slot map for routing but does not hand it out, and
//! `SCAN` has no slot to route by — it is a per-node walk. So a scan asks
//! `CLUSTER SHARDS` which primaries exist and visits each one by address.
//! Asked live rather than cached: a migration changes the answer, and a stale
//! list would skip a primary that now owns keys.

use redis::Value;
use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};

use crate::client::Dragonfly;
use crate::error::{self, Result};

const CMD_CLUSTER: &str = "CLUSTER";
const ARG_SHARDS: &str = "SHARDS";
const ARG_SLOTS: &str = "SLOTS";
const FIELD_NODES: &str = "nodes";
const FIELD_IP: &str = "ip";
const FIELD_ENDPOINT: &str = "endpoint";
const FIELD_PORT: &str = "port";
const FIELD_ROLE: &str = "role";
const ROLE_MASTER: &str = "master";

/// Where a node listens, as the cluster announces it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeAddress {
    pub(crate) host: String,
    pub(crate) port: u16,
}

/// What a node does for its shard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// Owns the shard's slots and takes every write.
    Primary,
    /// Follows a primary; never addressed by this crate, counted by the
    /// capacity report because a shard with none has no failover.
    Replica,
}

/// One node the cluster names, with what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) address: NodeAddress,
    pub(crate) role: Role,
}

/// Every node the cluster currently names, in shard order.
///
/// # Errors
/// Returns a command error when the cluster refuses the question and an
/// unexpected-reply error when the answer is not the documented shape.
pub(crate) async fn nodes(redis: &Dragonfly) -> Result<Vec<Node>> {
    let mut cmd = redis::cmd(CMD_CLUSTER);
    cmd.arg(ARG_SHARDS);
    let shards: Value = redis.command(CMD_CLUSTER, ARG_SHARDS, &cmd).await?;
    nodes_in(shards)
}

/// Every node a `CLUSTER SHARDS` reply names. A reply that is not a list of
/// shards is refused rather than read as an empty cluster.
fn nodes_in(shards: Value) -> Result<Vec<Node>> {
    let Value::Array(shards) = shards else {
        return Err(error::unexpected_reply(CMD_CLUSTER));
    };
    let mut found = Vec::new();
    for shard in shards {
        found.extend(nodes_of(shard)?.into_iter().filter_map(node_of));
    }
    Ok(found)
}

/// Every primary the cluster currently names.
///
/// # Errors
/// As [`nodes`].
pub(crate) async fn primaries(redis: &Dragonfly) -> Result<Vec<NodeAddress>> {
    Ok(nodes(redis)
        .await?
        .into_iter()
        .filter(|node| node.role == Role::Primary)
        .map(|node| node.address)
        .collect())
}

/// One contiguous run of slots and the primary serving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SlotRange {
    pub(crate) first: u16,
    pub(crate) last: u16,
    /// The primary's node id, the answer it gives to `CLUSTER MYID`; `None`
    /// when the reply leaves it out.
    pub(crate) id: Option<String>,
}

/// The slot map as `CLUSTER SLOTS` states it, asked on `connection`.
///
/// `SLOTS` rather than `SHARDS` because it is the reply the driver builds its
/// own routing from: a range here is the range the driver routes by.
///
/// # Errors
/// The driver's error when no node answers.
pub(crate) async fn slot_ranges(
    connection: &mut ClusterConnection,
) -> redis::RedisResult<Vec<SlotRange>> {
    let mut cmd = redis::cmd(CMD_CLUSTER);
    cmd.arg(ARG_SLOTS);
    let routing = RoutingInfo::SingleNode(SingleNodeRoutingInfo::Random);
    Ok(ranges_in(&connection.route_command(cmd, routing).await?))
}

/// Every well-formed range a `CLUSTER SLOTS` reply names. A malformed range
/// is skipped rather than guessed at: a slot no range claims is a channel the
/// caller cannot place, which it treats as a loss it redials, never as a
/// subscription it sends somewhere arbitrary.
fn ranges_in(reply: &Value) -> Vec<SlotRange> {
    let Value::Array(ranges) = reply else {
        return Vec::new();
    };
    ranges.iter().filter_map(range_of).collect()
}

/// `[first, last, [host, port, id, …], replicas…]`, or `None` for another
/// shape.
fn range_of(range: &Value) -> Option<SlotRange> {
    let Value::Array(fields) = range else {
        return None;
    };
    let mut fields = fields.iter();
    let first = small(fields.next()?)?;
    let last = small(fields.next()?)?;
    let Value::Array(primary) = fields.next()? else {
        return None;
    };
    // `[host, port, id, …]`: the address is read only to reject a shape
    // that is not a node, because nothing here routes by it.
    let mut primary = primary.iter();
    text(primary.next()?)?;
    small(primary.next()?)?;
    let id = primary.next().and_then(text);
    Some(SlotRange { first, last, id })
}

/// An integer reply that fits a slot number or a port.
fn small(value: &Value) -> Option<u16> {
    match value {
        Value::Int(number) => u16::try_from(*number).ok(),
        _other => None,
    }
}

fn nodes_of(shard: Value) -> Result<Vec<Value>> {
    match field(shard, FIELD_NODES) {
        Some(Value::Array(nodes)) => Ok(nodes),
        _missing => Err(error::unexpected_reply(CMD_CLUSTER)),
    }
}

/// A node's address and role, or `None` for an entry missing either.
///
/// Any role the cluster spells other than `master` is a replica: the only
/// question this crate asks is "may I write here", and the answer is the same
/// for a replica, a syncing replica and anything a future release adds.
fn node_of(node: Value) -> Option<Node> {
    let entries = pairs(node)?;
    let role = entries
        .iter()
        .find_map(|(key, value)| (key == FIELD_ROLE).then(|| shown(value)))?;
    let host = entries.iter().find_map(|(key, value)| {
        (key == FIELD_IP || key == FIELD_ENDPOINT).then(|| shown(value))
    })?;
    let port = entries.iter().find_map(|(key, value)| match value {
        Value::Int(port) if key == FIELD_PORT => u16::try_from(*port).ok(),
        _other => None,
    })?;
    let role = if role == ROLE_MASTER {
        Role::Primary
    } else {
        Role::Replica
    };
    Some(Node {
        address: NodeAddress { host, port },
        role,
    })
}

/// One named field of a map reply, under either RESP3 map or RESP2 flat
/// framing — Dragonfly answers this command as a flat array of pairs.
fn field(value: Value, name: &str) -> Option<Value> {
    pairs(value)?
        .into_iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn pairs(value: Value) -> Option<Vec<(String, Value)>> {
    match value {
        Value::Map(entries) => Some(
            entries
                .into_iter()
                .map(|(key, value)| (shown(&key), value))
                .collect(),
        ),
        Value::Array(flat) => {
            let mut out = Vec::with_capacity(flat.len() / 2);
            let mut items = flat.into_iter();
            while let (Some(key), Some(value)) = (items.next(), items.next()) {
                out.push((shown(&key), value));
            }
            Some(out)
        }
        _other => None,
    }
}

/// The text of a RESP string value, or `None` when it is not one.
///
/// The crate's single reading of a `Value` as text. `hub::pump` had its own,
/// which matched `BulkString` and `SimpleString` and dropped
/// `VerbatimString` -- a shape RESP3 is exactly the protocol to send, and the
/// hub speaks RESP3. Two matches meant the second could go on missing a case
/// the first already handled.
pub(crate) fn text(value: &Value) -> Option<String> {
    match value {
        Value::BulkString(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::SimpleString(text) | Value::VerbatimString { text, .. } => Some(text.clone()),
        _other => None,
    }
}

/// The same reading, rendered for a field this module always has to show.
///
/// A topology row is diagnostic output: a value that is not a string is still
/// worth printing as itself rather than vanishing.
fn shown(value: &Value) -> String {
    text(value).unwrap_or_else(|| format!("{value:?}"))
}

#[cfg(test)]
mod tests;
