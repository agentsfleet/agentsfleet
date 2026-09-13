//! What boot refuses before any work is accepted.
//!
//! Three questions, asked of the datastore itself and answered before the
//! first write: is this a cluster, does it speak the commands this daemon's
//! design requires, and does it keep every key it is given. A no to any of
//! them is a deployment that would lose accepted work or silently drop a
//! surface, and the only honest moment to say so is before a producer has
//! been told yes.
//!
//! # Why eviction is a boot refusal and not a runtime warning
//!
//! Every key this crate writes is accepted work or the record of it. A stream
//! entry is a receipt the ledger re-appends if it vanishes; a readiness mark,
//! a pending list and a session have no second copy at all. A node in
//! `cache_mode`, or under any `maxmemory_policy` but `noeviction`, discards
//! keys when it is full and then answers every read as though they had never
//! existed. That is data loss the datastore performs on purpose, and the only
//! moment to refuse it is before the first write — so the check runs beside
//! the connect and answers in the same error class, and a daemon that boots
//! has proven every primary keeps what it is given.
//!
//! # Asked of every primary, and only primaries
//!
//! `INFO memory` has no key, so it is routed per primary the way every other
//! server statistic is. Replicas are not asked: a replica evicting keys is a
//! replica that will disagree with its primary, which a failover surfaces and
//! nothing here could prevent.

use redis::Value;

use crate::client::Redis;
use crate::error::{ErrorKind, Result};

/// The `INFO` section that reports what the server IS, and the command that
/// asks for it.
///
/// `INFO cluster` and not `CLUSTER INFO`: the two are different replies, and
/// only this one carries the field below. Measured against Dragonfly v1.40.2
/// on the local four-node rig — `CLUSTER INFO` answers sixteen fields
/// beginning `cluster_state:ok` and names `cluster_enabled` in none of them,
/// while `INFO cluster` answers `cluster_enabled:1`. Reading the wrong one
/// refused boot on a healthy cluster.
const CMD_INFO: &str = "INFO";
const SECTION_CLUSTER: &str = "cluster";

/// The field `INFO cluster` reports cluster mode under, and the one value
/// that means it is on.
const FIELD_CLUSTER_ENABLED: &str = "cluster_enabled";
const CLUSTER_IS_ENABLED: &str = "1";

/// `COMMAND`'s subcommand.
const ARG_INFO: &str = "INFO";

/// What a missing `cluster_enabled` field is reported as. A server that
/// does not say cannot be taken to have said yes.
const CLUSTER_MODE_UNSTATED: &str = "unstated";

/// The command that reports what the server can do.
const CMD_COMMAND: &str = "COMMAND";

/// The command this daemon's live tail is built on. Sharded pub/sub routes
/// a channel by its own slot; the unsharded pair broadcasts every publish
/// to every node, which is the cost this design exists to avoid.
const CMD_SSUBSCRIBE: &str = "SSUBSCRIBE";

/// The `INFO` section that carries both settings.
const SECTION_MEMORY: &str = "memory";

/// Redis's eviction policy, as `INFO memory` spells it, and the one value
/// under which nothing is evicted.
const FIELD_MAXMEMORY_POLICY: &str = "maxmemory_policy";
const POLICY_NO_EVICTION: &str = "noeviction";

/// Dragonfly's own switch for the same behaviour, and how it spells "on".
///
/// `cache` and not `true`: the field is not a boolean. Measured on Dragonfly
/// v1.40.2, a node keeping every key reports `cache_mode:store` and the
/// `--cache_mode` flag's own help text pairs it with `cache`. A node in cache
/// mode also flips `maxmemory_policy` to `eviction`, so the policy arm below
/// refuses it either way — but it refuses it naming the wrong setting, and a
/// value this field can never hold is a check that is not one.
const FIELD_CACHE_MODE: &str = "cache_mode";
const CACHE_MODE_ON: &str = "cache";

/// Refuses a datastore this daemon must not accept work on.
///
/// The order is the order an operator can act on: what the datastore IS
/// first, then what it can do, then how it is configured. A seed that is
/// not a cluster makes the other two questions moot, and asking them first
/// would report a missing command on a server that was never the right
/// server.
///
/// # Errors
/// Returns the first refusal found — see [`refuse_non_cluster`],
/// [`refuse_missing_sharded_pubsub`] and [`refuse_eviction`] — and whatever
/// asking the datastore returns.
pub async fn refuse_unsuitable_datastore(redis: &Redis) -> Result<()> {
    refuse_non_cluster(redis).await?;
    refuse_missing_sharded_pubsub(redis).await?;
    refuse_eviction(redis).await
}

/// Refuses a seed that answers as a single server.
///
/// Asked of the server rather than inferred from whether the driver managed
/// to build a slot map: the driver's behaviour against a standalone is the
/// driver's business and has changed between releases, where
/// `cluster_enabled` is the server's own documented answer to exactly this
/// question. One node is asked because every node gives the same answer.
///
/// # Errors
/// Returns a not-a-cluster error naming what was reported, and a command
/// error when the datastore will not answer.
pub async fn refuse_non_cluster(redis: &Redis) -> Result<()> {
    let mut cmd = redis::cmd(CMD_INFO);
    cmd.arg(SECTION_CLUSTER);
    let reply: String = redis.ask_one_node(CMD_INFO, SECTION_CLUSTER, &cmd).await?;
    let reported = field_of(&reply, FIELD_CLUSTER_ENABLED).unwrap_or(CLUSTER_MODE_UNSTATED);
    if reported == CLUSTER_IS_ENABLED {
        return Ok(());
    }
    Err(ErrorKind::NotACluster {
        reported: reported.to_owned(),
    }
    .into())
}

/// Refuses a server that does not know `SSUBSCRIBE`.
///
/// # Errors
/// Returns a missing-capability error, and a command error when the
/// datastore will not answer.
pub async fn refuse_missing_sharded_pubsub(redis: &Redis) -> Result<()> {
    let mut cmd = redis::cmd(CMD_COMMAND);
    cmd.arg(ARG_INFO).arg(CMD_SSUBSCRIBE);
    let reply: Value = redis
        .ask_one_node(CMD_COMMAND, CMD_SSUBSCRIBE, &cmd)
        .await?;
    if knows_command(&reply) {
        return Ok(());
    }
    Err(ErrorKind::MissingCapability {
        command: CMD_SSUBSCRIBE,
    }
    .into())
}

/// Whether a `COMMAND INFO` reply says the server knows what was asked
/// about.
///
/// Only a reply that AFFIRMATIVELY says no — one entry, nil, which is how
/// the protocol spells "never heard of it" — is read as a missing command.
/// A framing this does not recognise is read as PRESENT, because refusing
/// boot is the heavier of the two answers and "this server lacks the
/// command" is a claim the server has to have made. A live tail that turns
/// out to be unsupported degrades to carrying no frames, which the hub
/// already does; a daemon that refuses to boot serves nothing at all.
fn knows_command(reply: &Value) -> bool {
    match reply {
        Value::Array(entries) => entries
            .first()
            .is_some_and(|entry| !matches!(entry, Value::Nil)),
        _unrecognised_framing => true,
    }
}

/// The value of one `key:value` field in an `INFO`-shaped reply.
///
/// Every `INFO` section answers the same way: one field per line, name and
/// value split at the first colon, section headers (`# Memory`) carrying no
/// colon and so contributing nothing.
fn field_of<'reply>(reply: &'reply str, field: &str) -> Option<&'reply str> {
    reply
        .lines()
        .filter_map(|line| line.trim_end().split_once(':'))
        .find_map(|(name, value)| (name == field).then_some(value))
}

/// Refuses a cluster any of whose primaries evicts keys.
///
/// # Errors
/// Returns an unsafe-eviction error naming the first primary found evicting
/// and the setting that said so, a command error when a primary will not
/// answer `INFO`, and whatever reading the topology returns.
pub async fn refuse_eviction(redis: &Redis) -> Result<()> {
    redis
        .info_per_primary(SECTION_MEMORY)
        .await?
        .iter()
        .enumerate()
        .find_map(|(node, reply)| eviction_in(reply).map(|setting| (node, setting)))
        .map_or(Ok(()), |(node, setting)| {
            Err(ErrorKind::UnsafeEviction { node, setting }.into())
        })
}

/// The eviction setting an `INFO memory` reply admits to, spelled
/// `field=value` for the refusal, or `None` when the node keeps every key.
///
/// A reply carrying neither field is a node that keeps every key: Redis
/// always reports its policy, and Dragonfly reports its cache switch, so the
/// absence of both is a build that has no eviction to report rather than one
/// hiding it.
fn eviction_in(reply: &str) -> Option<String> {
    reply
        .lines()
        .filter_map(|line| line.trim_end().split_once(':'))
        .find_map(|(field, value)| {
            let evicts = match field {
                FIELD_MAXMEMORY_POLICY => value != POLICY_NO_EVICTION,
                FIELD_CACHE_MODE => value == CACHE_MODE_ON,
                _keeps_every_key => false,
            };
            evicts.then(|| format!("{field}={value}"))
        })
}

#[cfg(test)]
mod tests;
