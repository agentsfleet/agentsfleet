//! The directory's decisions that need no datastore: the order a poll is
//! offered dirty partitions in, which entries it skips, and that a command
//! which never answers gives up at its deadline.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use afd_core::clock::UnixMillis;
use afd_core::test_util::trace::Capture;
use afd_crypto::entropy::Entropy;

use super::{
    EVENT_HINT_WRITE_FAILED, HINT_DEADLINE, Partition, ReadyToken, oldest_first, report_unwritten,
    within,
};
use crate::error::{self, Result};

/// A deadline short enough that a test waiting it out costs nothing.
const SHORT: Duration = Duration::from_millis(5);

/// The command a deadline test names.
const COMMAND: &str = "HGETALL";

/// Two instants a millisecond apart, so their tokens sort by mint order.
const EARLIER: UnixMillis = UnixMillis::from_millis(1_759_000_000_000);
const LATER: UnixMillis = UnixMillis::from_millis(1_759_000_000_001);

fn token(at: UnixMillis) -> ReadyToken {
    ReadyToken::mint(&Entropy::new(), at).expect("the host has entropy")
}

/// Two partitions dirtied in turn come back in that turn, whatever order the
/// hash returned them in.
#[test]
fn test_dirty_partitions_come_back_oldest_first() {
    let (older, newer) = (token(EARLIER), token(LATER));
    let entries = HashMap::from([
        ("7".to_owned(), newer.as_str().to_owned()),
        ("2".to_owned(), older.as_str().to_owned()),
    ]);

    let dirty = oldest_first(entries);

    let partitions: Vec<u16> = dirty.iter().map(|entry| entry.partition.index()).collect();
    assert_eq!(
        partitions,
        [2, 7],
        "the older mark's partition is offered first"
    );
    assert_eq!(dirty.first().map(|entry| &entry.token), Some(&older));
}

/// A field that names no partition is skipped: past the last, or not a
/// number at all.
#[test]
fn test_an_entry_naming_no_partition_is_skipped() {
    let entries = HashMap::from([
        ("16".to_owned(), token(EARLIER).as_str().to_owned()),
        ("north".to_owned(), token(EARLIER).as_str().to_owned()),
        ("3".to_owned(), token(LATER).as_str().to_owned()),
    ]);

    let dirty = oldest_first(entries);

    assert_eq!(dirty.len(), 1, "{dirty:?}");
    assert_eq!(
        dirty.first().map(|entry| entry.partition),
        Partition::new(3)
    );
}

/// A command that never answers gives up at its deadline, as the timeout the
/// caller already knows how to absorb.
#[tokio::test]
async fn test_a_hint_that_never_answers_returns_at_its_deadline() {
    let started = Instant::now();

    let outcome = within(SHORT, COMMAND, std::future::pending::<Result<()>>()).await;

    let failure = outcome.expect_err("a command that never answers is a failure");
    assert!(failure.is_unavailable(), "{failure}");
    assert!(
        failure.to_string().contains(COMMAND),
        "the command that hung is named: {failure}"
    );
    assert!(
        started.elapsed() < HINT_DEADLINE * 10,
        "the wait was the deadline, not the datastore's: {:?}",
        started.elapsed()
    );
}

/// An answer inside the deadline passes through untouched, failures included.
#[tokio::test]
async fn test_an_answer_inside_the_deadline_passes_through() {
    let answered = within(HINT_DEADLINE, COMMAND, async {
        Ok::<_, crate::Error>(3_u8)
    })
    .await;
    assert_eq!(answered.expect("answered"), 3);

    let refused = within(HINT_DEADLINE, COMMAND, async {
        Err::<u8, _>(error::unexpected_reply(COMMAND))
    })
    .await;
    assert!(refused.is_err(), "a refusal stays a refusal");
}

/// The production deadline is a fraction of a poll interval, so giving up on
/// the hint never costs a poll more than the hint could have saved.
#[test]
fn test_the_hint_deadline_is_inside_one_poll_interval() {
    let interval = Duration::from_millis(u64::from(afd_core::timing::NO_WORK_RETRY_AFTER_MS));
    assert!(HINT_DEADLINE < interval / 4, "{HINT_DEADLINE:?}");
}

/// A lost directory write is reported once, naming its partition and the
/// datastore's own code, so an operator can tell a sick slot from a bug.
#[test]
fn test_an_unwritten_entry_is_reported_with_its_partition() {
    let capture = Capture::install();
    let partition = Partition::new(9).expect("9 is a partition");

    report_unwritten(
        partition,
        &error::timed_out(COMMAND, HINT_DEADLINE.as_millis()),
    );

    let line = capture.only(EVENT_HINT_WRITE_FAILED);
    assert_eq!(line.field("partition"), Some("9"));
    assert!(
        line.field("cause_error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        line.field("reason")
            .is_some_and(|reason| reason.contains(COMMAND))
    );
}

/// Over a datastore nobody answers on, each directory command fails in the
/// way its caller absorbs: the write is reported where it fails and never
/// raised, and the read and the delete hand their failure back.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_a_directory_nobody_answers_costs_only_the_hint() {
    use crate::{Dragonfly, DragonflyConfig, DragonflyRole};

    /// Port 1 is reserved and unbound.
    const NOWHERE: &str = "redis://127.0.0.1:1";

    let capture = Capture::install();
    let queue = Dragonfly::unreachable(&DragonflyConfig::from_url(
        DragonflyRole::Default,
        NOWHERE.to_owned(),
    ))
    .expect("a lazy handle opens no socket");
    let index = super::ReadyIndex::new(queue);
    let partition = Partition::new(4).expect("4 is a partition");
    let dirty = super::Dirty {
        partition,
        token: token(EARLIER),
    };

    index.note_dirty(partition, &dirty.token).await;
    let line = capture.only(EVENT_HINT_WRITE_FAILED);
    assert_eq!(line.field("partition"), Some("4"));

    assert!(
        index.dirty().await.is_err(),
        "the read hands its failure back"
    );
    assert!(index.spend(&dirty).await.is_err(), "and so does the delete");
}
