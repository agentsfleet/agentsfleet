//! A checkpoint's custody as the engine reads it: what the caller is
//! answered with is the custody's own first failure, never the engine's
//! account of it.

use std::sync::{Mutex, PoisonError};

use af_chunk_store::{ChunkSource as _, Resolved};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use zeroize::Zeroizing;

use super::{Custody, DETAIL_ABSENT, DETAIL_OTHER, DaemonSource, land};
use crate::test_support::{self, Lander};
use crate::{Result, error};

const CHECKPOINT: &str = "019f0000-0000-7000-8000-000000000001";
const OTHER: &str = "019f0000-0000-7000-8000-000000000002";

/// A custody whose tree, when it has one, names one file, and whose every
/// read fails naming `read`.
#[derive(Debug)]
struct Failing {
    tree: Option<&'static str>,
    read: &'static str,
}

impl Custody for Failing {
    fn tree(&mut self) -> impl Future<Output = Result<Resolved>> + Send {
        std::future::ready(
            self.tree
                .map_or_else(named, |detail| Err(error::config(detail))),
        )
    }

    fn read(
        &mut self,
        _objects: &[String],
    ) -> impl Future<Output = Result<Vec<Zeroizing<Vec<u8>>>>> + Send {
        std::future::ready(Err(error::config(self.read)))
    }
}

/// A custody that answers each object its own name, or nothing at all, and
/// keeps every window it was asked for.
#[derive(Debug, Default)]
struct Answering {
    nothing: bool,
    windows: Vec<Vec<String>>,
}

impl Custody for Answering {
    fn tree(&mut self) -> impl Future<Output = Result<Resolved>> + Send {
        std::future::ready(named())
    }

    fn read(
        &mut self,
        objects: &[String],
    ) -> impl Future<Output = Result<Vec<Zeroizing<Vec<u8>>>>> + Send {
        self.windows.push(objects.to_vec());
        let named = objects.iter().filter(|_| !self.nothing);
        let read = named.map(|object| Zeroizing::new(object.as_bytes().to_vec()));
        std::future::ready(Ok(read.collect()))
    }
}

/// A tree naming one file of one object.
fn named() -> Result<Resolved> {
    let mut tree = Resolved::default();
    tree.apply(CheckpointEntry::File(CheckpointFile {
        path: "notes".to_owned(),
        executable: false,
        size: 5,
        objects: vec![OTHER.to_owned()],
    }))?;
    Ok(tree)
}

/// A source over `custody`, reading the fixture checkpoint.
fn source<C: Custody>(custody: C) -> DaemonSource<C> {
    DaemonSource {
        checkpoint: Uuid7::parse(CHECKPOINT).unwrap_or_else(|_| unreachable!("a fixed identifier")),
        generation: 1,
        custody: tokio::sync::Mutex::new(custody),
        failure: Mutex::new(None),
    }
}

/// What `source` kept as its first failure.
fn kept<C>(source: DaemonSource<C>) -> Option<&'static str> {
    let kept = source
        .failure
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    test_support::detail(&kept.map_or(Ok(()), Err))
}

/// A read the custody failed is what the restore answers, and nothing lands.
#[tokio::test]
async fn a_custody_failure_is_what_the_restore_answers() -> Result<()> {
    let lander = Lander::on(&"a".repeat(64)).await;
    let custody = Failing {
        tree: None,
        read: "the object would not read",
    };

    let checkpoint = (Uuid7::parse(CHECKPOINT)?, 1);
    let restored = land(custody, checkpoint, lander.environment(), lander.landing()).await;

    assert_eq!(
        test_support::detail(&restored),
        Some("the object would not read")
    );
    assert_eq!(std::fs::read_dir(lander.workspace())?.count(), 0);
    Ok(())
}

/// The first failure is kept and a later one dropped; the engine is told
/// only that the source could not answer, under the failure's code.
#[tokio::test]
async fn the_first_custody_failure_is_kept() -> Result<()> {
    let checkpoint = Uuid7::parse(CHECKPOINT)?;
    let failing = source(Failing {
        tree: Some("the manifest would not read"),
        read: "the object would not read",
    });

    let told = failing.tree(&checkpoint).await.err();
    let later = failing.read(OTHER).await.err();

    let code = error::config(DETAIL_OTHER).code();
    assert_eq!(told.map(|told| told.code()), Some(code));
    assert!(later.is_some());
    assert_eq!(kept(failing), Some("the manifest would not read"));
    Ok(())
}

/// After its tree, a window reaches the custody whole, as one read, and
/// answers each object in the order asked; a single object is a window of
/// one.
#[tokio::test]
async fn a_window_is_one_read_of_the_custody() -> Result<()> {
    let reading = source(Answering::default());
    let window = ["b", "a", "b", "c"].map(str::to_owned);

    let stored = reading.tree(&Uuid7::parse(CHECKPOINT)?).await.ok();
    let read = reading.read_window(&window).await.ok();
    let one = reading.read("d").await.ok();

    let read: Option<Vec<Vec<u8>>> =
        read.map(|read| read.iter().map(|chunk| chunk.to_vec()).collect());
    let expected = [b"b", b"a", b"b", b"c"].map(|chunk| chunk.to_vec());
    assert_eq!(read, Some(expected.to_vec()));
    assert_eq!(one.map(|chunk| chunk.to_vec()), Some(b"d".to_vec()));
    let paths = stored.map(|stored| stored.tree.into_entries().into_keys().collect());
    assert_eq!(paths, Some(vec!["notes".to_owned()]));
    let windows = reading.custody.into_inner().windows;
    assert_eq!(windows, [window.to_vec(), vec!["d".to_owned()]]);
    Ok(())
}

/// A single read the custody answers with no plaintext is refused, and kept
/// as the restore's failure.
#[tokio::test]
async fn a_read_the_custody_answers_with_nothing_is_refused() -> Result<()> {
    let reading = source(Answering {
        nothing: true,
        ..Answering::default()
    });

    let refused = reading.read(OTHER).await.err();

    let code = error::config(DETAIL_ABSENT).code();
    assert_eq!(refused.map(|refused| refused.code()), Some(code));
    assert_eq!(kept(reading), Some(DETAIL_ABSENT));
    Ok(())
}

/// A tree asked for under another identifier is refused before the custody
/// reads anything, and its debug form names only the checkpoint.
#[tokio::test]
async fn a_tree_under_another_identifier_is_refused() -> Result<()> {
    let asked = source(Failing {
        tree: None,
        read: "unread",
    });

    let refused = asked.tree(&Uuid7::parse(OTHER)?).await.err();
    let shown = format!("{asked:?}");

    assert!(refused.is_some());
    assert_eq!(
        shown,
        format!("DaemonSource {{ checkpoint: Uuid7({CHECKPOINT:?}), .. }}")
    );
    assert_eq!(kept(asked), Some(DETAIL_OTHER));
    Ok(())
}
