//! Trees crossing in chunks: each import spooled on the workspace disk under
//! a number, each export packed as it is read, its chunks on the blocking
//! pool. A save's pin walk is an export of what changed since the last
//! committed pin.

use std::fs::File;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use jsonrpsee_types::{Id, Request};

use super::Session;
use super::params;
use crate::server::files::Workspace;
use af_guest_wire::error::{self, Result};
use af_guest_wire::protocol::{
    ExportChunk, ExportOpenParams, ExportReadParams, ImportOpenParams, ImportWriteParams,
    METHOD_EXPORT_OPEN, METHOD_EXPORT_READ, METHOD_IMPORT_COMMIT, METHOD_IMPORT_OPEN,
    METHOD_IMPORT_WRITE, METHOD_PIN_RELEASE, METHOD_PIN_WALK, METHOD_TRANSFER_DROP,
    PinReleaseParams, PinWalkParams, PinWalked, TransferOpened, TransferParams,
    transfer_chunk_bytes,
};
use af_guest_wire::tree::PinWalk;

/// A transfer the session does not hold: never opened, finished, or given up.
const DETAIL_NO_TRANSFER: &str = "no transfer with that identifier";
/// An export read that does not start where the last one ended.
const DETAIL_OUT_OF_ORDER: &str = "an export is read in order, from where the last read ended";

/// One tree crossing, held until it is committed, read to its end or given
/// up, or the connection closes.
pub(super) enum Transfer {
    /// A tar stream arriving, to unpack under `at`.
    Import { spool: Arc<File>, at: PathBuf },
    /// A tar stream being packed, read in order.
    Export { stream: Arc<Mutex<Exporting>> },
}

/// An export's chunks as the packer hands them over, and how far they have
/// been read.
pub(super) struct Exporting {
    chunks: Receiver<Result<Vec<u8>>>,
    offset: u64,
}

impl Session {
    /// Runs the transfer call `request` names; `false` when it names none.
    pub(super) fn transfer(&mut self, id: Id<'static>, request: &Request<'_>) -> bool {
        match request.method.as_ref() {
            METHOD_IMPORT_OPEN => {
                let opened = params(request).and_then(|open| self.open_import(&open));
                self.answer(id, opened);
            }
            METHOD_IMPORT_WRITE => self.write_import(id, params(request)),
            METHOD_IMPORT_COMMIT => self.commit_import(id, params(request)),
            METHOD_EXPORT_OPEN => self.open_export(id, params(request)),
            METHOD_EXPORT_READ => self.read_export(id, params(request)),
            METHOD_PIN_WALK => self.pin_walk(id, params(request)),
            METHOD_PIN_RELEASE => self.pin_release(id, params(request)),
            METHOD_TRANSFER_DROP => {
                let dropped = params(request).and_then(|forget: TransferParams| {
                    self.transfers
                        .remove(&forget.transfer_id)
                        .map(drop)
                        .ok_or_else(no_transfer)
                });
                self.answer(id, dropped);
            }
            _other => return false,
        }
        true
    }

    /// Forgets the transfer a call on the blocking pool finished, if any; a
    /// call that panicked names none, and its spool goes with the connection.
    pub(super) fn forget(&mut self, finished: Option<u64>) {
        if let Some(transfer) = finished {
            self.transfers.remove(&transfer);
        }
    }

    /// The next transfer number, holding `transfer` under it.
    fn hold(&mut self, transfer: Transfer) -> u64 {
        let transfer_id = self.next_transfer;
        self.next_transfer += 1;
        self.transfers.insert(transfer_id, transfer);
        transfer_id
    }

    /// Opens a spool for an import under `open.at`, refused at once when the
    /// path leaves the workspace.
    fn open_import(&mut self, open: &ImportOpenParams<'static>) -> Result<TransferOpened> {
        let at = self.workspace.import_root(&open.at)?;
        let spool = Arc::new(self.workspace.spool()?);
        let transfer_id = self.hold(Transfer::Import { spool, at });
        Ok(TransferOpened { transfer_id })
    }

    /// Writes one chunk of an import into its spool.
    fn write_import(&mut self, id: Id<'static>, params: Result<ImportWriteParams>) {
        match params.and_then(|write| Ok((self.import_spool(write.transfer_id)?, write))) {
            Ok((spool, write)) => self.blocking(id, move |_workspace| {
                let written = Workspace::write_chunk(&spool, write.offset, &write.data);
                (written, None)
            }),
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }

    /// Checks and unpacks an import; it is forgotten whatever the outcome.
    fn commit_import(&mut self, id: Id<'static>, params: Result<TransferParams>) {
        let taken = params.and_then(|commit| match self.transfers.remove(&commit.transfer_id) {
            Some(Transfer::Import { spool, at }) => Ok((spool, at)),
            Some(other) => {
                self.transfers.insert(commit.transfer_id, other);
                Err(no_transfer())
            }
            None => Err(no_transfer()),
        });
        match taken {
            Ok((spool, at)) => {
                self.blocking(id, move |workspace| (workspace.import(&at, &spool), None));
            }
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }

    /// Starts packing an export, answered with its number at once; the pack
    /// runs only as far ahead as its reader.
    fn open_export(&mut self, id: Id<'static>, params: Result<ExportOpenParams<'static>>) {
        let most = transfer_chunk_bytes(self.sizes.max_frame_bytes);
        let opened = params.and_then(|open| self.workspace.export(&open.paths, most));
        let answered = opened.map(|chunks| {
            let stream = Arc::new(Mutex::new(Exporting { chunks, offset: 0 }));
            TransferOpened {
                transfer_id: self.hold(Transfer::Export { stream }),
            }
        });
        self.answer(id, answered);
    }

    /// Reads the next chunk of an export, which must start where the last
    /// one ended; the chunk that ends it, or a failure, forgets it.
    fn read_export(&mut self, id: Id<'static>, params: Result<ExportReadParams>) {
        let found = params.and_then(|read| match self.transfers.get(&read.transfer_id) {
            Some(Transfer::Export { stream }) => Ok((Arc::clone(stream), read)),
            Some(Transfer::Import { .. }) | None => Err(no_transfer()),
        });
        match found {
            Ok((stream, read)) => self.blocking(id, move |_workspace| {
                let mut stream = stream.lock().unwrap_or_else(PoisonError::into_inner);
                let next = if read.offset == stream.offset {
                    stream
                        .chunks
                        .recv()
                        .unwrap_or_else(|_closed| Ok(Vec::new()))
                } else {
                    Err(error::invalid_params(DETAIL_OUT_OF_ORDER))
                };
                match next {
                    Ok(data) => {
                        let end = data.is_empty();
                        stream.offset = stream
                            .offset
                            .saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
                        let chunk = ExportChunk {
                            data: data.into(),
                            end,
                        };
                        (Ok(chunk), end.then_some(read.transfer_id))
                    }
                    Err(failure) => (Err(failure), Some(read.transfer_id)),
                }
            }),
            Err(failure) => self.answer::<ExportChunk>(id, Err(failure)),
        }
    }

    /// Walks the workspace for a save's pin and holds the export of what
    /// changed under a number of its own, answered with the walk; the export
    /// is read as `fs/export_read` reads one. The walk runs on the blocking
    /// pool; one that fails forgets its export.
    fn pin_walk(&mut self, id: Id<'static>, params: Result<PinWalkParams>) {
        let walk = match params {
            Ok(walk) => walk,
            Err(failure) => return self.answer::<PinWalked>(id, Err(failure)),
        };
        let (sender, chunks) = sync_channel(1);
        let stream = Arc::new(Mutex::new(Exporting { chunks, offset: 0 }));
        let transfer_id = self.hold(Transfer::Export { stream });
        let (pins, frame_bytes) = (Arc::clone(&self.pins), self.sizes.max_frame_bytes);
        self.blocking(id, move |workspace| {
            let walked = workspace.pin_walk(&pins, walk.parent.as_deref(), frame_bytes, sender);
            match walked {
                Ok(walked) => (Ok(answered(walked, transfer_id)), None),
                Err(failure) => (Err(failure), Some(transfer_id)),
            }
        });
    }

    /// Ends a pin, keeping its identities when it names the generation the
    /// daemon committed from it.
    fn pin_release(&mut self, id: Id<'static>, params: Result<PinReleaseParams>) {
        let pins = Arc::clone(&self.pins);
        self.on_files(id, params, move |_workspace, release: PinReleaseParams| {
            pins.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .release(release.pin, release.generation.as_deref())
        });
    }

    /// The spool of import `transfer_id`.
    fn import_spool(&self, transfer_id: u64) -> Result<Arc<File>> {
        match self.transfers.get(&transfer_id) {
            Some(Transfer::Import { spool, .. }) => Ok(Arc::clone(spool)),
            Some(Transfer::Export { .. }) | None => Err(no_transfer()),
        }
    }
}

/// A pin's walk as its answer names it, its export held as `transfer_id`.
fn answered(walked: PinWalk, transfer_id: u64) -> PinWalked {
    PinWalked {
        pin: walked.pin,
        transfer_id,
        delta: walked.delta,
        removed: walked.removed,
        entries: walked.entries,
        changed_bytes: walked.changed_bytes,
    }
}

/// The refusal for a transfer the session does not hold.
fn no_transfer() -> error::Error {
    error::invalid_params(DETAIL_NO_TRANSFER)
}
