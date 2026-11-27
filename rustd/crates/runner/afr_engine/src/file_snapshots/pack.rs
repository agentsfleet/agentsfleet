//! A pin's tar stream read as the changes a store publishes, and a saved tree
//! packed back into the tar stream a guest agent's import takes.
//!
//! ```text
//!   pin spool ─► entries in stream order, each file's chunks after it
//!   saved tree ─► directory · link · file (its objects read in order) ─► spool
//! ```
//!
//! The stream is read on a blocking thread and handed across a one-slot
//! channel, so a reader holds at most a chunk or two of plaintext. A second
//! name of a file, packed as a link to its first, is read again from the
//! first: each name is saved as a file of its own.

use std::convert::identity;
use std::fs::File;
use std::io::{self, Read, Seek as _, Write as _};
use std::path::{Component, Path, PathBuf};

use af_chunk_store::{Change, ChunkKey, ChunkSource, Resolved};
use af_wire::durable_task::{CHECKPOINT_OBJECT_BYTES, CheckpointEntry, CheckpointFile};
use futures_util::{StreamExt as _, stream::BoxStream};
use tar::{Archive, Builder, EntryType, Header};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::byte_count;
use crate::error::Result;

/// The permission bits that make a file executable.
const EXECUTABLE: u32 = 0o111;
/// The modes a restored entry lands with: a tree saves only whether a file
/// is executable.
const DIRECTORY_MODE: u32 = 0o755;
const FILE_MODE: u32 = 0o644;
const EXECUTABLE_MODE: u32 = 0o755;
const LINK_MODE: u32 = 0o777;
/// Why a stream or a saved tree is not one a restore lands.
const NOT_RELATIVE: &str = "an entry's name leaves the workspace or is not UTF-8";
const UNKNOWN_LINK: &str = "a second name links to no file packed before it";
const SHORT_FILE: &str = "a file's bytes do not add up to its size";

/// The stream's reader, and the thread reading it.
type Reading = (mpsc::Receiver<Change>, JoinHandle<Result<()>>);

/// Every change the tar stream at `spool` holds, from the first, the content
/// its files claim held to `room`; the stream ends with the read's failure,
/// if it had one.
pub(super) fn changes(spool: PathBuf, room: u64) -> BoxStream<'static, Result<Change>> {
    let (send, receive) = mpsc::channel(1);
    let reading = tokio::task::spawn_blocking(move || {
        read(&spool, room, &mut |change| {
            send.blocking_send(change).is_ok()
        })
    });
    futures_util::stream::unfold(Some((receive, reading)), next).boxed()
}

/// The read's next change; once it is done, its failure, if it had one.
async fn next(state: Option<Reading>) -> Option<(Result<Change>, Option<Reading>)> {
    let (mut receive, reading) = state?;
    if let Some(change) = receive.recv().await {
        return Some((Ok(change), Some((receive, reading))));
    }
    // A read that panicked ends the stream as one that failed.
    let read = reading.await.map_err(crate::Error::from).and_then(identity);
    read.err().map(|failure| (Err(failure), None))
}

/// Hands `emit` each entry of the stream at `spool`, each file's chunks
/// after it; stops, with no error, once `emit` answers that nothing reads on.
/// A stream whose files claim more than `room`, holes included, is refused:
/// every byte read is a chunk a store takes.
fn read(spool: &Path, room: u64, emit: &mut dyn FnMut(Change) -> bool) -> Result<()> {
    let mut archive = Archive::new(File::open(spool)?);
    let mut claimed = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        claimed = claimed.saturating_add(entry.size());
        if claimed > room {
            return Err(io::Error::from(io::ErrorKind::FileTooLarge).into());
        }
        let path = relative(&entry.path()?)?;
        let kind = entry.header().entry_type();
        let reads_on = match kind {
            EntryType::Directory => emit(Change::Entry(CheckpointEntry::Directory { path })),
            EntryType::Symlink => {
                let target = named(entry.link_name()?.as_deref())?.to_owned();
                emit(Change::Entry(CheckpointEntry::Symlink { path, target }))
            }
            EntryType::Link => {
                let first = relative(Path::new(named(entry.link_name()?.as_deref())?))?;
                again(spool, &first, &path, emit)?
            }
            EntryType::Regular | EntryType::GNUSparse | EntryType::Continuous => {
                let executable = entry.header().mode()? & EXECUTABLE != 0;
                file(&mut entry, &path, executable, emit)?
            }
            // A socket, a pipe or a device: nothing a tree carries.
            _other => true,
        };
        if !reads_on {
            return Ok(());
        }
    }
    Ok(())
}

/// Reads the file packed first as `first` again, saved as `path`.
fn again(
    spool: &Path,
    first: &str,
    path: &str,
    emit: &mut dyn FnMut(Change) -> bool,
) -> Result<bool> {
    let mut archive = Archive::new(File::open(spool)?);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_file() && relative(&entry.path()?)? == first {
            let executable = entry.header().mode()? & EXECUTABLE != 0;
            return file(&mut entry, path, executable, emit);
        }
    }
    Err(invalid(UNKNOWN_LINK))
}

/// Hands `emit` the file entry `content` is, then its chunks; whether a
/// reader still reads on.
fn file<R: Read>(
    content: &mut tar::Entry<'_, R>,
    path: &str,
    executable: bool,
    emit: &mut dyn FnMut(Change) -> bool,
) -> Result<bool> {
    let size = content.size();
    let listed = CheckpointFile {
        path: path.to_owned(),
        executable,
        size,
        objects: Vec::new(),
    };
    if !emit(Change::Entry(CheckpointEntry::File(listed))) {
        return Ok(false);
    }
    let (mut index, mut left) = (0, size);
    while left > 0 {
        let span = left.min(CHECKPOINT_OBJECT_BYTES);
        let mut bytes = Zeroizing::new(Vec::with_capacity(usize::try_from(span).unwrap_or(0)));
        content.by_ref().take(span).read_to_end(&mut bytes)?;
        if byte_count(bytes.len()) != span {
            return Err(invalid(SHORT_FILE));
        }
        let key = ChunkKey::File {
            path: path.to_owned(),
            index,
        };
        if !emit(Change::Chunk { key, bytes }) {
            return Ok(false);
        }
        (index, left) = (index + 1, left - span);
    }
    Ok(true)
}

/// Packs `tree` into a fresh spool under `spools`, each file's objects read
/// from `source` in order, and hands the spool back from its start.
///
/// # Errors
/// An object would not read, a file's objects do not add up to its size, or
/// the spool would not write.
pub(super) async fn pack(tree: &Resolved, source: &dyn ChunkSource, spools: &Path) -> Result<File> {
    let mut builder = Builder::new(tempfile::tempfile_in(spools)?);
    for (path, entry) in tree.entries() {
        let mut header = Header::new_gnu();
        header.set_size(0);
        match entry {
            CheckpointEntry::Directory { .. } => {
                header.set_entry_type(EntryType::Directory);
                header.set_mode(DIRECTORY_MODE);
                builder.append_data(&mut header, path, io::empty())?;
            }
            CheckpointEntry::Symlink { target, .. } => {
                header.set_entry_type(EntryType::Symlink);
                header.set_mode(LINK_MODE);
                builder.append_link(&mut header, path, target)?;
            }
            CheckpointEntry::File(file) => {
                let mut content = gather(file, source, spools).await?;
                header.set_entry_type(EntryType::Regular);
                header.set_mode(if file.executable {
                    EXECUTABLE_MODE
                } else {
                    FILE_MODE
                });
                header.set_size(file.size);
                builder.append_data(&mut header, path, &mut content)?;
            }
            // A resolved tree has applied every removal.
            CheckpointEntry::Remove { .. } => {}
        }
    }
    let mut spool = builder.into_inner()?;
    spool.rewind()?;
    Ok(spool)
}

/// `file`'s content, its objects read from `source` in order into a spool
/// under `spools`, handed back from its start.
async fn gather(file: &CheckpointFile, source: &dyn ChunkSource, spools: &Path) -> Result<File> {
    let mut content = tempfile::tempfile_in(spools)?;
    let mut gathered = 0;
    for object in &file.objects {
        let bytes = source.read(object).await?;
        content.write_all(&bytes)?;
        gathered += byte_count(bytes.len());
    }
    if gathered != file.size {
        return Err(invalid(SHORT_FILE));
    }
    content.rewind()?;
    Ok(content)
}

/// `path` as a workspace-relative name, its parts joined by `/`.
fn relative(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                parts.push(part.to_str().ok_or_else(|| invalid(NOT_RELATIVE))?);
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(invalid(NOT_RELATIVE));
            }
        }
    }
    if parts.is_empty() {
        return Err(invalid(NOT_RELATIVE));
    }
    Ok(parts.join("/"))
}

/// A link's target as UTF-8.
fn named(target: Option<&Path>) -> Result<&str> {
    target
        .and_then(Path::to_str)
        .ok_or_else(|| invalid(NOT_RELATIVE))
}

/// A stream or a tree no restore lands, saying why.
fn invalid(why: &'static str) -> crate::Error {
    io::Error::new(io::ErrorKind::InvalidData, why).into()
}

#[cfg(test)]
#[path = "tests/pack.rs"]
mod tests;
