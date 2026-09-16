//! Reading a `CLUSTER SHARDS` reply, in both framings and every malformed
//! shape the cluster can answer with.
//!
//! The parsing here is the crate's only reading of a topology, and every
//! branch below is a reply a live cluster or a proxy in front of one can
//! actually send: RESP3 map framing, RESP2 flat pairs, a node missing the
//! field this crate addresses it by. None of it needs a datastore, and none of
//! it was reachable through the one live-cluster test that reads a real reply.

use super::*;

fn bulk(text: &str) -> Value {
    Value::BulkString(text.as_bytes().to_vec())
}

/// Dragonfly answers RESP2 flat pairs, a RESP3 server answers a map, and this
/// crate must read a shard the same way either way — the parse is one
/// function precisely so the two cannot drift.
#[test]
fn a_shard_reads_the_same_under_map_and_flat_framing() {
    let entries = vec![
        (bulk(FIELD_ROLE), bulk(ROLE_MASTER)),
        (bulk(FIELD_IP), bulk("10.0.0.7")),
        (bulk(FIELD_PORT), Value::Int(6379)),
    ];
    let flat = Value::Array(
        entries
            .iter()
            .flat_map(|(key, value)| [key.clone(), value.clone()])
            .collect(),
    );
    let map = Value::Map(entries);

    let expected = Node {
        address: NodeAddress {
            host: "10.0.0.7".to_owned(),
            port: 6379,
        },
        role: Role::Primary,
    };
    assert_eq!(node_of(map), Some(expected.clone()));
    assert_eq!(node_of(flat), Some(expected));
}

/// Any role the cluster spells other than `master` is a replica — the crate
/// asks only whether it may write, and the answer is the same for a replica, a
/// syncing one, and whatever a future release names.
#[test]
fn every_role_but_master_reads_as_a_replica() {
    for spelling in ["replica", "slave", "syncing-something-new"] {
        let node = Value::Array(vec![
            bulk(FIELD_ROLE),
            bulk(spelling),
            bulk(FIELD_ENDPOINT),
            bulk("shard-2.internal"),
            bulk(FIELD_PORT),
            Value::Int(6380),
        ]);
        let read = node_of(node).expect("a node naming endpoint, port and role reads");
        assert_eq!(
            read.role,
            Role::Replica,
            "role {spelling} read as a primary"
        );
        assert_eq!(read.address.host, "shard-2.internal");
    }
}

/// A node this crate cannot address is dropped rather than guessed at: a
/// missing port, an unreadable one, or no role at all.
#[test]
fn a_node_missing_what_addresses_it_is_not_returned() {
    let no_port = Value::Array(vec![
        bulk(FIELD_ROLE),
        bulk(ROLE_MASTER),
        bulk(FIELD_IP),
        bulk("10.0.0.7"),
    ]);
    assert_eq!(node_of(no_port), None);

    let port_too_large = Value::Array(vec![
        bulk(FIELD_ROLE),
        bulk(ROLE_MASTER),
        bulk(FIELD_IP),
        bulk("10.0.0.7"),
        bulk(FIELD_PORT),
        Value::Int(i64::from(u16::MAX) + 1),
    ]);
    assert_eq!(node_of(port_too_large), None);

    let no_role = Value::Array(vec![
        bulk(FIELD_IP),
        bulk("10.0.0.7"),
        bulk(FIELD_PORT),
        Value::Int(6379),
    ]);
    assert_eq!(node_of(no_role), None);

    // Not a map and not an array: nothing to read pairs out of at all.
    assert_eq!(node_of(Value::Int(1)), None);
}

/// A shard reply with no `nodes` field is the cluster answering a shape this
/// crate does not know — an error naming the command, not an empty list that
/// would read as a cluster with no primaries.
#[test]
fn a_shard_without_a_nodes_field_is_an_unexpected_reply() {
    let shard = Value::Array(vec![
        bulk("slots"),
        Value::Array(vec![Value::Int(0), Value::Int(16383)]),
    ]);
    let refused = nodes_of(shard).expect_err("a shard with no nodes field is refused");
    assert!(
        refused.to_string().contains(CMD_CLUSTER),
        "the refusal should name the command that produced it: {refused}"
    );

    // Present but not an array — the same refusal, for the same reason.
    let wrong_shape = Value::Array(vec![bulk(FIELD_NODES), bulk("not-a-list")]);
    assert!(nodes_of(wrong_shape).is_err());
}

/// The crate's single reading of a value as text. `VerbatimString` is the case
/// a hand-rolled second reading dropped, and RESP3 is exactly the protocol
/// that sends it.
#[test]
fn text_reads_every_string_framing_and_nothing_else() {
    assert_eq!(text(&bulk("plain")).as_deref(), Some("plain"));
    assert_eq!(
        text(&Value::SimpleString("simple".to_owned())).as_deref(),
        Some("simple")
    );
    assert_eq!(
        text(&Value::VerbatimString {
            format: redis::VerbatimFormat::Text,
            text: "verbatim".to_owned(),
        })
        .as_deref(),
        Some("verbatim")
    );
    assert_eq!(text(&Value::Int(7)), None);
    assert_eq!(text(&Value::Nil), None);
}

/// A topology row is diagnostic output, so a field that is not a string still
/// prints as itself rather than vanishing from the row.
#[test]
fn a_non_string_field_is_shown_rather_than_dropped() {
    assert_eq!(shown(&bulk("text")), "text");
    assert_eq!(shown(&Value::Int(6379)), format!("{:?}", Value::Int(6379)));
}
