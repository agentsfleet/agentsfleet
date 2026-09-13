//! What boot refuses before any work is accepted: a primary that evicts.
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

use crate::client::Redis;
use crate::error::{ErrorKind, Result};

/// The `INFO` section that carries both settings.
const SECTION_MEMORY: &str = "memory";

/// Redis's eviction policy, as `INFO memory` spells it, and the one value
/// under which nothing is evicted.
const FIELD_MAXMEMORY_POLICY: &str = "maxmemory_policy";
const POLICY_NO_EVICTION: &str = "noeviction";

/// Dragonfly's own switch for the same behaviour, and how it spells "on".
const FIELD_CACHE_MODE: &str = "cache_mode";
const CACHE_MODE_ON: &str = "true";

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
mod tests {
    use super::eviction_in;

    /// A Redis reply under `noeviction`, and a Dragonfly reply with the cache
    /// switch off, both keep every key.
    #[test]
    fn a_node_that_keeps_every_key_is_not_refused() {
        let redis = "# Memory\r\nused_memory:1024\r\nmaxmemory_policy:noeviction\r\n";
        let dragonfly = "# Memory\r\nused_memory:1024\r\ncache_mode:false\r\n";
        assert_eq!(eviction_in(redis), None);
        assert_eq!(eviction_in(dragonfly), None);
        assert_eq!(eviction_in("# Memory\r\nused_memory:1024\r\n"), None);
    }

    /// Either switch, on, is named in the refusal — with its value, so the
    /// operator reads which policy to change rather than that one exists.
    #[test]
    fn an_evicting_node_is_named_with_its_setting() {
        assert_eq!(
            eviction_in("maxmemory_policy:allkeys-lru\r\n").as_deref(),
            Some("maxmemory_policy=allkeys-lru")
        );
        assert_eq!(
            eviction_in("used_memory:1\r\ncache_mode:true\r\n").as_deref(),
            Some("cache_mode=true")
        );
    }
}
