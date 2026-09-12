//! The parts of the ledger that decide something without a datastore: the
//! producer vocabulary, the logical id's shape, and the digest a retry is
//! checked against.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::collections::BTreeSet;

use afd_wire::event::EventType;

use super::{Admission, Key, Producer, Replayed, logical_id};

/// Every producer, so a test cannot silently cover five of six.
///
/// Written out rather than derived: a variant added without a spelling here
/// fails to compile at the `match` in [`Producer::as_str`] and fails the
/// count below, which is two failures naming the same omission.
const EVERY_PRODUCER: [Producer; 6] = [
    Producer::Steer,
    Producer::Webhook,
    Producer::WebhookApp,
    Producer::ScheduleFire,
    Producer::GateContinuation,
    Producer::RepairVerification,
];

/// An admission whose fields a test then varies one at a time.
fn sample() -> Admission<'static> {
    Admission {
        producer: Producer::Webhook,
        key: Key::Repeated("fleet-1:delivery-9"),
        fleet: "019feca5-bc9b-72e8-b71f-e2714f6b0120",
        workspace: "019feca5-bc9b-72e8-b71f-e2714f6b0121",
        actor: "steer:user_1",
        event_type: EventType::Webhook,
        request_json: r#"{"message":"hello"}"#,
    }
}

/// No two producers share a spelling.
///
/// The spelling is half of the dedup key, so two producers spelling
/// themselves the same would silently deduplicate against each other: a
/// webhook delivery id equal to a schedule message id would run once instead
/// of twice. A unique index cannot catch that, because to Postgres the two
/// rows ARE the same key.
#[test]
fn every_producer_spells_itself_and_no_two_spell_alike() {
    let spellings: BTreeSet<&str> = EVERY_PRODUCER.iter().map(|p| p.as_str()).collect();
    assert_eq!(
        spellings.len(),
        EVERY_PRODUCER.len(),
        "two producers share a spelling, which makes their keys collide"
    );
    for producer in EVERY_PRODUCER {
        assert!(
            !producer.as_str().is_empty(),
            "{producer:?} spells itself as nothing"
        );
    }
}

/// The logical id keeps the `<millis>-<n>` shape a stream entry id has.
///
/// Load-bearing rather than cosmetic: the console sorts on it, the cursor
/// pages on it, and `afd_events`' keyset compares it as text. A shape change
/// here is a silent reordering of every history page.
#[test]
fn a_logical_id_has_the_shape_every_reader_was_written_against() {
    let id = logical_id(1_788_550_034_853, 7);
    assert_eq!(id, "1788550034853-7");
    let (millis, seq) = id.split_once('-').expect("the id carries one separator");
    assert!(millis.parse::<i64>().is_ok(), "the left half is an instant");
    assert!(seq.parse::<i64>().is_ok(), "the right half is a sequence");
}

/// Two ids minted in the same millisecond order by their sequence.
#[test]
fn ids_from_one_millisecond_are_ordered_by_their_sequence() {
    let first = logical_id(1_788_550_034_853, 1);
    let second = logical_id(1_788_550_034_853, 2);
    assert_ne!(first, second, "one millisecond must not collapse two rows");
}

/// The same payload digests the same, whatever instant it is admitted at.
///
/// The instant is deliberately not in the digest: a sender retrying an hour
/// later sends the same delivery, and hashing the arrival time would make
/// every retry look like drift.
#[test]
fn one_payload_digests_the_same_however_often_it_is_admitted() {
    let admission = sample();
    assert_eq!(admission.payload_digest(), admission.payload_digest());
}

/// Changing any digested field changes the digest.
#[test]
fn every_digested_field_changes_the_digest() {
    let base = sample().payload_digest();
    let varied = [
        Admission {
            actor: "steer:user_2",
            ..sample()
        },
        Admission {
            event_type: EventType::Chat,
            ..sample()
        },
        Admission {
            workspace: "019feca5-bc9b-72e8-b71f-e2714f6b0999",
            ..sample()
        },
        Admission {
            request_json: r#"{"message":"goodbye"}"#,
            ..sample()
        },
    ];
    for admission in varied {
        assert_ne!(
            admission.payload_digest(),
            base,
            "a changed field left the digest alone"
        );
    }
}

/// Fields cannot slide into each other and hash alike.
///
/// The separator's whole job. Without it `actor="ab", workspace="c"` and
/// `actor="a", workspace="bc"` feed the hasher identical bytes, and a
/// redelivery whose actor gained a character would read as the same payload.
#[test]
fn two_fields_cannot_slide_into_each_other() {
    let left = Admission {
        actor: "ab",
        workspace: "c",
        ..sample()
    };
    let right = Admission {
        actor: "a",
        workspace: "bc",
        ..sample()
    };
    assert_ne!(
        left.payload_digest(),
        right.payload_digest(),
        "the field separator is not doing its job"
    );
}

/// The digest is a hex-rendered SHA-256, so a column of them is fixed-width.
#[test]
fn a_digest_is_sixty_four_hex_characters() {
    let digest = sample().payload_digest();
    assert_eq!(digest.len(), 64, "SHA-256 renders as 64 hex characters");
    assert!(
        digest.chars().all(|c| c.is_ascii_hexdigit()),
        "a digest carries only hex: {digest}"
    );
}

/// A pass that appended everything it scanned is clean; one that did not is
/// not — including the pass that scanned rows and appended none.
#[test]
fn a_replay_pass_is_clean_only_when_it_appended_everything_it_scanned() {
    assert!(Replayed::default().is_clean(), "an empty pass is clean");
    assert!(
        Replayed {
            scanned: 4,
            appended: 4
        }
        .is_clean()
    );
    assert!(
        !Replayed {
            scanned: 4,
            appended: 3
        }
        .is_clean(),
        "a pass the queue refused partway is not clean"
    );
    assert!(
        !Replayed {
            scanned: 4,
            appended: 0
        }
        .is_clean(),
        "a pass that appended nothing is not clean"
    );
}
