#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;

use af_chunk_store::{Change, ChunkSource as _, ChunkStore as _, MemoryStore, Resolved};
use af_wire::durable_task::{CHECKPOINT_OBJECT_BYTES, CheckpointEntry, CheckpointFile};
use futures_util::StreamExt as _;
use tar::{Builder, EntryType, Header};

use super::{changes, named, pack, read, relative};
use crate::engine::fresh_sandbox_id;

/// A tar stream of `entries`, each `(name, type, content)`, its names
/// written raw so a test can name one no builder would.
fn stream(entries: &[(&str, EntryType, &[u8])]) -> Vec<u8> {
    let mut builder = Builder::new(Vec::new());
    for (name, kind, content) in entries {
        let mut header = Header::new_gnu();
        let raw = header.as_old_mut().name.iter_mut();
        raw.zip(name.bytes()).for_each(|(slot, byte)| *slot = byte);
        header.set_entry_type(*kind);
        header.set_mode(0o755);
        header.set_size(u64::try_from(content.len()).unwrap());
        if *kind == EntryType::Link || *kind == EntryType::Symlink {
            header.set_size(0);
            header
                .set_link_name(std::str::from_utf8(content).unwrap())
                .unwrap();
        }
        header.set_cksum();
        let data: &[u8] = if header.size().unwrap() == 0 {
            &[]
        } else {
            content
        };
        builder.append(&header, data).unwrap();
    }
    builder.into_inner().unwrap()
}

/// `bytes` written to a fresh file; the file and where it is.
fn spooled(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut spool = tempfile::NamedTempFile::new().unwrap();
    spool.write_all(bytes).unwrap();
    spool
}

/// What reading `spool` hands on, until `stop_after` changes when given.
fn read_all(spool: &Path, stop_after: Option<usize>) -> crate::Result<Vec<Change>> {
    let mut seen = Vec::new();
    read(spool, u64::MAX, &mut |change| {
        seen.push(change);
        stop_after.is_none_or(|stop| seen.len() < stop)
    })?;
    Ok(seen)
}

fn entry(change: &Change) -> Option<&CheckpointEntry> {
    match change {
        Change::Entry(entry) => Some(entry),
        Change::Chunk { .. } => None,
    }
}

/// A file past one chunk is handed on as two, after its entry; a directory,
/// a link and a second name are entries, the second name read again from the
/// first; a pipe is nothing; and a reader that stops is not read further.
#[tokio::test]
async fn test_a_pin_reads_as_entries_with_each_files_chunks_after_it() {
    let big = vec![1; usize::try_from(CHECKPOINT_OBJECT_BYTES).unwrap() + 1];
    let spool = spooled(&stream(&[
        ("./dir/", EntryType::Directory, b""),
        ("dir/big", EntryType::Regular, &big),
        ("dir/link", EntryType::Symlink, b"big"),
        ("dir/same", EntryType::Link, b"dir/big"),
        ("pipe", EntryType::Fifo, b""),
    ]));

    let streamed: Vec<Change> = changes(spool.path().to_owned(), u64::MAX)
        .map(Result::unwrap)
        .collect()
        .await;
    let stopped = read_all(spool.path(), Some(2)).unwrap();
    let stopped_in_a_chunk = read_all(spool.path(), Some(3)).unwrap();
    let stopped_in_a_second_name = read_all(spool.path(), Some(5)).unwrap();

    let entries: Vec<&CheckpointEntry> = streamed.iter().filter_map(entry).collect();
    let paths: Vec<&str> = entries.iter().map(|entry| entry.path()).collect();
    assert_eq!(paths, ["dir", "dir/big", "dir/link", "dir/same"]);
    assert_eq!(
        streamed.len(),
        8,
        "four entries and two files of two chunks"
    );
    let Some(CheckpointEntry::File(same)) = entries.get(3) else {
        unreachable!("a second name is a file")
    };
    assert!(same.executable && same.size == u64::try_from(big.len()).unwrap());
    assert_eq!(stopped.len(), 2);
    assert_eq!(stopped_in_a_chunk.len(), 3);
    assert_eq!(stopped_in_a_second_name.len(), 5);
}

/// A name leaving the workspace, a second name of nothing packed and a file
/// shorter than its size each end the read with a failure; a dropped reader
/// ends it with none.
#[tokio::test]
async fn test_a_pin_no_walk_packs_is_refused() {
    for refused in [
        stream(&[("/etc/passwd", EntryType::Regular, b"x")]),
        stream(&[("../out", EntryType::Regular, b"x")]),
        stream(&[("dir/same", EntryType::Link, b"missing")]),
    ] {
        read_all(spooled(&refused).path(), None).unwrap_err();
    }
    let mut short = stream(&[("cut", EntryType::Regular, &[3; 600])]);
    short.truncate(512 + 100);
    let cut = spooled(&short);
    let cut_read = changes(cut.path().to_owned(), u64::MAX)
        .collect::<Vec<_>>()
        .await;
    assert!(cut_read.last().unwrap().is_err());
    let claimed = spooled(&stream(&[("big", EntryType::Regular, &[1; 9])]));
    read(claimed.path(), 8, &mut |_change| true).unwrap_err();
    relative(Path::new(".")).unwrap_err();
    named(None).unwrap_err();
}

/// The tree a store saved for one executable file, `run`, with a
/// directory, a link and a removal set beside it.
async fn saved(store: &MemoryStore) -> BTreeMap<String, CheckpointEntry> {
    let id = fresh_sandbox_id();
    let mut publication = store.publish(&id, None).await.unwrap();
    let spool = spooled(&stream(&[("run", EntryType::Regular, b"#!/bin/sh\n")]));
    for change in read_all(spool.path(), None).unwrap() {
        publication.add(change).await.unwrap();
    }
    publication.commit().await.unwrap();
    let mut entries = store.tree(&id).await.unwrap().tree.into_entries();
    let (d, l, x) = ("d".to_owned(), "l".to_owned(), "x".to_owned());
    entries.insert(d.clone(), CheckpointEntry::Directory { path: d });
    let target = "run".to_owned();
    entries.insert(l.clone(), CheckpointEntry::Symlink { path: l, target });
    entries.insert(x.clone(), CheckpointEntry::Remove { path: x });
    entries
}

/// Each entry `packed` holds, by its path: its type and mode.
fn landed(packed: &mut std::fs::File) -> BTreeMap<String, (EntryType, u32)> {
    let mut archive = tar::Archive::new(packed);
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let header = entry.header();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            (path, (header.entry_type(), header.mode().unwrap()))
        })
        .collect()
}

/// A saved tree packs into the stream an import takes: each file's objects
/// read in order, its executable bit kept, and a removal packing nothing; a
/// file whose objects fall short of its size is refused.
#[tokio::test]
async fn test_a_saved_tree_packs_into_an_import_stream() {
    let store = MemoryStore::default();
    let mut entries = saved(&store).await;
    let spools = tempfile::tempdir().unwrap();

    let mut packed = pack(&Resolved::from(entries.clone()), &store, spools.path())
        .await
        .unwrap();

    let landed = landed(&mut packed);
    let kinds = ["run", "d", "l"].map(|path| landed.get(path).copied());
    assert_eq!(
        kinds,
        [
            Some((EntryType::Regular, 0o755)),
            Some((EntryType::Directory, 0o755)),
            Some((EntryType::Symlink, 0o777)),
        ]
    );
    assert_eq!(landed.len(), 3);
    let short = CheckpointFile {
        path: "short".to_owned(),
        executable: false,
        size: 9,
        objects: Vec::new(),
    };
    entries.insert("short".to_owned(), CheckpointEntry::File(short));
    pack(&Resolved::from(entries), &store, spools.path())
        .await
        .unwrap_err();
}
