//! What a reply destination does to an admission's digest and its binds.
//!
//! Split from the ledger's other datastore-free cases at the file cap: these
//! all turn on the destination an admission carries, which arrived after every
//! other digested field, so the property each proves is that the rows stored
//! before it still hash as they did.

use afd_core::id::Uuid7;

use super::*;

/// The digest [`sample`] had before admissions carried a destination.
///
/// Pinned as bytes rather than recomputed, because the property is about rows
/// already in the ledger: each was stored with the digest the four-part
/// formula gave it, and a retry arriving after this change must hash to the
/// same value or be logged as drift that never happened.
const DIGEST_BEFORE_DESTINATIONS: &str =
    "8b9567764bac9f4576acd8491bcf0c9ded10de18de4997503bccfa7bbb08b6fc";

/// A connector id as a producer owning a reply surface would state it.
const CONNECTOR: &str = "slack";

/// An event id as the ledger mints it: the admission row's `UUIDv7`.
const RESUMED: &str = "019feca5-bc9b-72e8-b71f-e2714f6b0177";

/// Two thread addresses that differ only in the thread.
const THREAD_A: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;
const THREAD_B: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000200"}"#;

/// No destination leaves the digest every earlier row was stored with.
#[test]
fn no_destination_keeps_the_digest_rows_were_stored_with() {
    assert_eq!(
        sample().reply,
        Reply::None,
        "the sample states no destination"
    );
    assert_eq!(sample().payload_digest(), DIGEST_BEFORE_DESTINATIONS);
}

/// A destination is part of what the event is, and its kind is tagged.
///
/// A retry naming another thread is drift worth a log line, so a changed
/// address must change the digest. The tag is what keeps a stated pair from
/// hashing like an inherited id: without it `To { "a", "b" }` and an
/// `Inherit` whose id happened to spell the same bytes would agree.
#[test]
fn every_destination_digests_apart() {
    let digests = [
        sample().payload_digest(),
        Admission {
            reply: Reply::To {
                connector: CONNECTOR,
                address: THREAD_A,
            },
            ..sample()
        }
        .payload_digest(),
        Admission {
            reply: Reply::To {
                connector: CONNECTOR,
                address: THREAD_B,
            },
            ..sample()
        }
        .payload_digest(),
        Admission {
            reply: Reply::Inherit { event_id: THREAD_A },
            ..sample()
        }
        .payload_digest(),
    ];
    let distinct: BTreeSet<&String> = digests.iter().collect();
    assert_eq!(
        distinct.len(),
        digests.len(),
        "two destinations hashed alike: {digests:?}"
    );
}

/// A reply reaches the statement as a stated pair or an inherited row, never both.
///
/// The statement `COALESCE`s a stated half with an inherited one, which is safe
/// only because this never sets both; and an id the ledger did not mint names
/// no row, so it binds as nothing at all rather than as a lookup that fails.
#[test]
fn a_reply_binds_at_most_one_pair() {
    let none = ReplyBinds::from(Reply::None);
    assert_eq!(
        (none.connector, none.address, none.inherited),
        (None, None, None)
    );

    let stated = ReplyBinds::from(Reply::To {
        connector: CONNECTOR,
        address: THREAD_A,
    });
    assert_eq!(
        (stated.connector, stated.address, stated.inherited),
        (Some(CONNECTOR), Some(THREAD_A), None)
    );

    let inherited = ReplyBinds::from(Reply::Inherit { event_id: RESUMED });
    assert_eq!(
        (
            inherited.connector,
            inherited.address,
            inherited.inherited.as_ref().map(Uuid7::as_str)
        ),
        (None, None, Some(RESUMED))
    );
}

/// An event id that is not a row id copies no destination, and never reaches
/// the statement's `::uuid` cast to be refused there — the retired
/// `<millis>-<n>` spelling included.
#[test]
fn an_id_that_names_no_row_inherits_nothing() {
    for foreign in ["not-a-ledger-id", "1700000000000-7", ""] {
        let binds = ReplyBinds::from(Reply::Inherit { event_id: foreign });
        assert_eq!(
            (binds.connector, binds.address, binds.inherited),
            (None, None, None),
            "{foreign:?} bound a destination"
        );
    }
}
