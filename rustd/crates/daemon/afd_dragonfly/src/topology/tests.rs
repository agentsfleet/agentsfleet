//! Reading a `CLUSTER SHARDS` reply, in both framings and every malformed
//! shape the cluster can answer with.
//!
//! The parsing here is the crate's only reading of a topology, and every
//! branch below is a reply a live cluster or a proxy in front of one can
//! actually send: RESP3 map framing, RESP2 flat pairs, a node missing the
//! field this crate addresses it by. None of it needs a datastore, and none of
//! it was reachable through the one live-cluster test that reads a real reply.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a unit test asserts by panicking; the crate's restriction set is for the daemon"
)]

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
    let Err(refused) = nodes_of(shard) else {
        panic!("a shard with no nodes field must be refused");
    };
    assert!(
        refused.to_string().contains(CMD_CLUSTER),
        "the refusal should name the command that produced it: {refused}"
    );

    // Present but not an array — the same refusal, for the same reason.
    let wrong_shape = Value::Array(vec![bulk(FIELD_NODES), bulk("not-a-list")]);
    assert!(
        matches!(nodes_of(wrong_shape), Err(_wrong_shape)),
        "a nodes field that is not a list must be refused too"
    );
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

/// A `CLUSTER SHARDS` answer that is not a list of shards is a refusal, not
/// an empty cluster: a scan that read it as "no primaries" would report every
/// key missing.
#[test]
fn a_shards_reply_that_is_not_a_list_is_an_unexpected_reply() {
    for reply in [Value::Okay, Value::Nil, bulk("shards"), Value::Int(4)] {
        let refused = nodes_in(reply.clone()).expect_err("not a list of shards");
        assert!(
            refused.to_string().contains(CMD_CLUSTER),
            "{reply:?} is refused naming the command: {refused}"
        );
    }
    assert_eq!(nodes_in(Value::Array(Vec::new())).ok(), Some(Vec::new()));
}

fn node_triple(host: &str, port: i64, id: &str) -> Value {
    Value::Array(vec![bulk(host), Value::Int(port), bulk(id)])
}

fn range(first: Value, last: Value, primary: Value) -> Value {
    Value::Array(vec![first, last, primary])
}

/// A `CLUSTER SLOTS` reply with malformed ranges keeps its well-formed ones
/// and drops each malformed one whole — never a range with a guessed bound or
/// a primary missing its address.
#[test]
fn a_slots_reply_keeps_its_well_formed_ranges_and_drops_the_rest() {
    let reply = Value::Array(vec![
        range(
            Value::Int(0),
            Value::Int(8_191),
            node_triple("10.0.0.1", 6379, "dfly-a"),
        ),
        Value::Int(7),
        range(Value::Int(8_192), Value::Int(16_383), Value::Int(6379)),
        range(
            bulk("zero"),
            Value::Int(9),
            node_triple("10.0.0.2", 6379, "dfly-b"),
        ),
        range(
            Value::Int(0),
            Value::Int(9),
            Value::Array(vec![bulk("10.0.0.3")]),
        ),
        range(
            Value::Int(0),
            Value::Int(70_000),
            node_triple("10.0.0.4", 6379, "dfly-c"),
        ),
        range(
            Value::Int(9),
            Value::Int(9),
            Value::Array(vec![bulk("10.0.0.5"), Value::Int(6379)]),
        ),
    ]);
    assert_eq!(
        ranges_in(&reply),
        vec![
            SlotRange {
                first: 0,
                last: 8_191,
                id: Some("dfly-a".to_owned()),
            },
            SlotRange {
                first: 9,
                last: 9,
                id: None,
            },
        ],
        "only the complete ranges survive; a node with no id is kept without one"
    );
}

/// A `CLUSTER SLOTS` answer that is not a list names no range at all.
#[test]
fn a_slots_reply_that_is_not_a_list_names_no_range() {
    for reply in [Value::Okay, Value::Nil, bulk("slots")] {
        assert!(ranges_in(&reply).is_empty(), "{reply:?}");
    }
}
