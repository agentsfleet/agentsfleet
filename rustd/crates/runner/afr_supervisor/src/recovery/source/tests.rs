//! A checkpoint's custody as the engine reads it: what the caller is
//! answered with is the custody's own first failure, never the engine's
//! account of it.

use std::sync::{Mutex, PoisonError};

use af_chunk_store::{ChunkSource as _, Resolved};
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointEntry, CheckpointFile};
use zeroize::Zeroizing;

use super::{Custody, DETAIL_OTHER, DaemonSource, land};
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

    fn read(&mut self, _object: &str) -> impl Future<Output = Result<Zeroizing<Vec<u8>>>> + Send {
        std::future::ready(Err(error::config(self.read)))
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
fn source(custody: Failing) -> DaemonSource<Failing> {
    DaemonSource {
        checkpoint: Uuid7::parse(CHECKPOINT).unwrap_or_else(|_| unreachable!("a fixed identifier")),
        generation: 1,
        custody: tokio::sync::Mutex::new(custody),
        failure: Mutex::new(None),
    }
}

/// What `source` kept as its first failure.
fn kept(source: DaemonSource<Failing>) -> Option<&'static str> {
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
