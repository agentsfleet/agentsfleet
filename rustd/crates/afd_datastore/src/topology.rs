//! What the cluster looks like right now, asked of the cluster itself.
//!
//! The driver keeps a slot map for routing but does not hand it out, and
//! `SCAN` has no slot to route by — it is a per-node walk. So a scan asks
//! `CLUSTER SHARDS` which primaries exist and visits each one by address.
//! Asked live rather than cached: a migration changes the answer, and a stale
//! list would skip a primary that now owns keys.

use redis::Value;

use crate::client::Redis;
use crate::error::{self, Result};

const CMD_CLUSTER: &str = "CLUSTER";
const ARG_SHARDS: &str = "SHARDS";
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

/// Every primary the cluster currently names.
///
/// # Errors
/// Returns a command error when the cluster refuses the question and an
/// unexpected-reply error when the answer is not the documented shape.
pub(crate) async fn primaries(redis: &Redis) -> Result<Vec<NodeAddress>> {
    let mut cmd = redis::cmd(CMD_CLUSTER);
    cmd.arg(ARG_SHARDS);
    let shards: Value = redis.command(CMD_CLUSTER, ARG_SHARDS, &cmd).await?;
    let Value::Array(shards) = shards else {
        return Err(error::unexpected_reply(CMD_CLUSTER));
    };
    let mut found = Vec::new();
    for shard in shards {
        for node in nodes_of(shard)? {
            if let Some(address) = primary_address(node) {
                found.push(address);
            }
        }
    }
    Ok(found)
}

fn nodes_of(shard: Value) -> Result<Vec<Value>> {
    match field(shard, FIELD_NODES) {
        Some(Value::Array(nodes)) => Ok(nodes),
        _missing => Err(error::unexpected_reply(CMD_CLUSTER)),
    }
}

/// A node's address when it is a primary, `None` when it is a replica.
fn primary_address(node: Value) -> Option<NodeAddress> {
    let entries = pairs(node)?;
    let role = entries
        .iter()
        .find_map(|(key, value)| (key == FIELD_ROLE).then(|| text(value)))?;
    if role != ROLE_MASTER {
        return None;
    }
    let host = entries
        .iter()
        .find_map(|(key, value)| (key == FIELD_IP || key == FIELD_ENDPOINT).then(|| text(value)))?;
    let port = entries.iter().find_map(|(key, value)| match value {
        Value::Int(port) if key == FIELD_PORT => u16::try_from(*port).ok(),
        _other => None,
    })?;
    Some(NodeAddress { host, port })
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
                .map(|(key, value)| (text(&key), value))
                .collect(),
        ),
        Value::Array(flat) => {
            let mut out = Vec::with_capacity(flat.len() / 2);
            let mut items = flat.into_iter();
            while let (Some(key), Some(value)) = (items.next(), items.next()) {
                out.push((text(&key), value));
            }
            Some(out)
        }
        _other => None,
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::BulkString(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Value::SimpleString(text) | Value::VerbatimString { text, .. } => text.clone(),
        other => format!("{other:?}"),
    }
}
