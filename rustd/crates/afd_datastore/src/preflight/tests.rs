//! The parts of preflight that decide something without a datastore: what an
//! `INFO`-shaped reply says, and what a `COMMAND INFO` reply admits to.

use redis::Value;

use super::{eviction_in, field_of, knows_command};

/// A Redis reply under `noeviction`, and a Dragonfly reply with the cache
/// switch off, both keep every key.
///
/// `cache_mode:store` is the real spelling, measured on Dragonfly v1.40.2:
/// the field is not a boolean, and the fixtures that said `false` and `true`
/// are why a check against `"true"` shipped unable to fire.
#[test]
fn a_node_that_keeps_every_key_is_not_refused() {
    let redis = "# Memory\r\nused_memory:1024\r\nmaxmemory_policy:noeviction\r\n";
    let dragonfly = "# Memory\r\nused_memory:1024\r\ncache_mode:store\r\n";
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
        eviction_in("used_memory:1\r\ncache_mode:cache\r\n").as_deref(),
        Some("cache_mode=cache")
    );
}

/// A boolean spelling of the cache switch is not a value Dragonfly emits, and
/// must not be read as eviction being off.
///
/// The regression this file owes. Dragonfly answers `store` or `cache`; a
/// fixture saying `false` made the old `"true"` comparison look tested while
/// it could never match a real node. `false` is not `cache`, so it keeps every
/// key — which is the right answer for the wrong reason, and the assertion
/// below pins the RIGHT reason: the only value that refuses is `cache`.
#[test]
fn only_dragonflys_own_spelling_of_the_cache_switch_refuses() {
    assert_eq!(eviction_in("cache_mode:store\r\n"), None);
    assert_eq!(
        eviction_in("cache_mode:cache\r\n").as_deref(),
        Some("cache_mode=cache")
    );
    // Neither boolean spelling is a value the server produces, so neither is
    // read as the switch being on.
    assert_eq!(eviction_in("cache_mode:true\r\n"), None);
    assert_eq!(eviction_in("cache_mode:false\r\n"), None);
}

/// The section preflight asks for what the server IS carries the field.
///
/// `INFO cluster`, not `CLUSTER INFO`. Measured on the live four-node rig:
/// `CLUSTER INFO` answers sixteen fields beginning `cluster_state:ok` and
/// names `cluster_enabled` in none of them, so reading it there refused boot
/// on a healthy cluster with `reported: "unstated"`.
#[test]
fn the_cluster_section_carries_the_field_and_a_shard_reply_does_not() {
    let info_cluster = "# Cluster\r\ncluster_enabled:1\r\n";
    assert_eq!(field_of(info_cluster, "cluster_enabled"), Some("1"));

    let cluster_info = "cluster_state:ok\r\ncluster_slots_assigned:16384\r\n\
                        cluster_known_nodes:4\r\ncluster_size:2\r\n";
    assert_eq!(
        field_of(cluster_info, "cluster_enabled"),
        None,
        "CLUSTER INFO does not carry it, which is the bug this pins"
    );
}

/// A field is read out of either reply shape, and an absent one is absent
/// rather than empty — the difference between a server that said zero and
/// one that said nothing.
#[test]
fn a_field_is_read_by_name_and_a_missing_one_is_none() {
    let reply = "cluster_enabled:1\r\ncluster_state:ok\r\ncluster_known_nodes:4\r\n";
    assert_eq!(field_of(reply, "cluster_enabled"), Some("1"));
    assert_eq!(field_of(reply, "cluster_state"), Some("ok"));
    assert_eq!(field_of(reply, "cluster_slots_assigned"), None);
    assert_eq!(
        field_of("cluster_enabled:0\r\n", "cluster_enabled"),
        Some("0")
    );
    // A name that is a prefix of another must not match it.
    assert_eq!(field_of("cluster_enabled_x:1\r\n", "cluster_enabled"), None);
}

/// A nil entry is the server saying it does not know the command; every
/// other framing is read as knowing it.
#[test]
fn only_a_nil_entry_is_read_as_a_missing_command() {
    assert!(!knows_command(&Value::Array(vec![Value::Nil])));
    assert!(!knows_command(&Value::Array(vec![])));
    assert!(knows_command(&Value::Array(vec![Value::SimpleString(
        "ssubscribe".to_owned()
    )])));
    // Fail OPEN on a shape this does not recognise: refusing boot needs the
    // server to have said no, and an unfamiliar framing has said nothing.
    assert!(knows_command(&Value::Nil));
    assert!(knows_command(&Value::Int(1)));
}
