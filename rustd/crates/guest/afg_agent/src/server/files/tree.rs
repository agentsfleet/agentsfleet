//! A tree crossing as a tar stream: unpacked under a workspace path, or
//! packed from workspace paths, through [`crate::tree`] over the workspace
//! handle.
//!
//! The stream waits in a spool, an anonymous file on the workspace disk that
//! no name reaches, so an import is checked whole before its first entry is
//! written.

use std::fs::File;
use std::io;
use std::os::unix::fs::FileExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Mutex, PoisonError};

use cap_tempfile::TempFile;

use super::Workspace;
use af_guest_wire::Imported;
use af_guest_wire::error::Result;
use af_guest_wire::protocol::transfer_chunk_bytes;
use af_guest_wire::tree::{self, PinWalk, Pins};

impl Workspace {
    /// A fresh spool on the workspace disk that no name reaches.
    pub(in crate::server) fn spool(&self) -> Result<File> {
        Ok(TempFile::new_anonymous(&self.dir)?.into_std())
    }

    /// Writes one chunk of an import's stream at `offset` in its spool.
    pub(in crate::server) fn write_chunk(spool: &File, offset: u64, data: &[u8]) -> Result<()> {
        Ok(spool.write_all_at(data, offset)?)
    }

    /// The workspace path an import unpacks under, refused before any chunk
    /// is sent when it leaves the workspace.
    pub(in crate::server) fn import_root(&self, at: &str) -> Result<PathBuf> {
        Ok(self.inside(at)?.to_owned())
    }

    /// Checks every entry of the stream in `spool`, then unpacks it under
    /// `at`, made when absent.
    pub(in crate::server) fn import(&self, at: &Path, spool: &File) -> Result<Imported> {
        tree::unpack(&self.dir, at, spool)
    }

    /// Packs `paths`, and everything below each, as a tar stream on a thread
    /// of its own, handed over in chunks of at most `chunk_bytes`. The
    /// channel holds one chunk, so the pack runs no further ahead than its
    /// reader and nothing is spooled: a workspace of any size exports into
    /// no space of its own. The stream ends when the channel closes; a pack
    /// that failed says why as its last item.
    ///
    /// # Errors
    /// A path that leaves the workspace, refused before anything is packed.
    pub(in crate::server) fn export(
        &self,
        paths: &[impl AsRef<str>],
        chunk_bytes: usize,
    ) -> Result<Receiver<Result<Vec<u8>>>> {
        let inside = paths
            .iter()
            .map(|path| self.inside(path.as_ref()).map(Path::to_owned))
            .collect::<Result<Vec<_>>>()?;
        let (sender, chunks) = sync_channel(1);
        self.pack_into(inside, chunk_bytes, sender)?;
        Ok(chunks)
    }

    /// Walks the workspace for a pin against `parent`, recording its
    /// identities in `pins`, and starts packing what changed into `sender`
    /// as [`Workspace::export`] packs. The answer it gives travels in a frame
    /// of `frame_bytes`.
    ///
    /// # Errors
    /// The walk could not read the workspace; nothing is packed or kept.
    pub(in crate::server) fn pin_walk(
        &self,
        pins: &Mutex<Pins>,
        parent: Option<&str>,
        frame_bytes: usize,
        sender: SyncSender<Result<Vec<u8>>>,
    ) -> Result<PinWalk> {
        let walked = pins.lock().unwrap_or_else(PoisonError::into_inner).walk(
            &self.dir,
            parent,
            frame_bytes,
        )?;
        self.pack_into(
            walked.roots.clone(),
            transfer_chunk_bytes(frame_bytes),
            sender,
        )?;
        Ok(walked)
    }

    /// Packs `inside`, paths already held inside the workspace, on a thread
    /// of its own into `sender`, at most `chunk_bytes` a chunk.
    fn pack_into(
        &self,
        inside: Vec<PathBuf>,
        chunk_bytes: usize,
        sender: SyncSender<Result<Vec<u8>>>,
    ) -> Result<()> {
        let dir = self.dir.try_clone()?;
        std::thread::Builder::new()
            .name(EXPORT_THREAD.to_owned())
            .spawn(move || {
                let mut writer = Chunks {
                    sender,
                    chunk_bytes,
                    buffer: Vec::with_capacity(chunk_bytes),
                };
                let borrowed = inside.iter().map(PathBuf::as_path).collect::<Vec<_>>();
                let packed = tree::pack(&dir, &borrowed, &mut writer)
                    .and_then(|()| io::Write::flush(&mut writer).map_err(Into::into));
                if let Err(failure) = packed {
                    // The reader may be gone; then nothing is owed.
                    let _unread = writer.sender.send(Err(failure));
                }
            })?;
        Ok(())
    }
}

/// The name of the thread that packs one export.
const EXPORT_THREAD: &str = "agent-export";

/// A writer that hands a tar stream on in chunks of at most `chunk_bytes`,
/// waiting while its reader has not taken the last one.
struct Chunks {
    sender: SyncSender<Result<Vec<u8>>>,
    chunk_bytes: usize,
    buffer: Vec<u8>,
}

impl io::Write for Chunks {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let room = self.chunk_bytes.saturating_sub(self.buffer.len());
        let taken = buf.len().min(room.max(1));
        self.buffer
            .extend_from_slice(buf.get(..taken).unwrap_or(buf));
        if self.buffer.len() >= self.chunk_bytes {
            self.flush()?;
        }
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let full = std::mem::replace(&mut self.buffer, Vec::with_capacity(self.chunk_bytes));
        self.sender
            .send(Ok(full))
            .map_err(|_gone| io::Error::from(io::ErrorKind::BrokenPipe))
    }
}
