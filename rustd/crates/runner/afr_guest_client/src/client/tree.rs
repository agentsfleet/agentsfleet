//! `import`, `export` and a pin's walk: a tar stream crossing the link a
//! chunk per frame.

use std::borrow::Cow;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use super::Client;
use af_guest_wire::Imported;
use af_guest_wire::error::Result;
use af_guest_wire::protocol::{
    ExportChunk, ExportOpenParams, ExportReadParams, ImportOpenParams, ImportWriteParams,
    METHOD_EXPORT_OPEN, METHOD_EXPORT_READ, METHOD_IMPORT_COMMIT, METHOD_IMPORT_OPEN,
    METHOD_IMPORT_WRITE, METHOD_PIN_WALK, METHOD_TRANSFER_DROP, PinWalkParams, PinWalked,
    TransferOpened, TransferParams, transfer_chunk_bytes,
};

impl Client {
    /// The most of a tree one frame of this link carries.
    fn chunk_bytes(&self) -> usize {
        transfer_chunk_bytes(self.sizes.max_frame_bytes)
    }

    /// Sends `tree` a chunk at a time, then commits it under `at`; a stream
    /// that fails to read gives the transfer up rather than leave its spool.
    pub(super) async fn import_tree(
        &self,
        at: &str,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<Imported> {
        let open = ImportOpenParams {
            at: Cow::Borrowed(at),
        };
        let TransferOpened { transfer_id } = self.fetch(METHOD_IMPORT_OPEN, &open).await?;
        let commit = TransferParams { transfer_id };
        match self.send_chunks(transfer_id, tree).await {
            Ok(()) => self.fetch(METHOD_IMPORT_COMMIT, &commit).await,
            Err(failure) => {
                self.give_up(transfer_id).await;
                Err(failure)
            }
        }
    }

    /// Writes `tree` into transfer `transfer_id`, each chunk at its offset.
    async fn send_chunks(
        &self,
        transfer_id: u64,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<()> {
        let chunk = self.chunk_bytes();
        let most = u64::try_from(chunk).unwrap_or(u64::MAX);
        let mut offset = 0_u64;
        loop {
            let mut data = Vec::with_capacity(chunk);
            let sent = (&mut *tree).take(most).read_to_end(&mut data).await?;
            if sent == 0 {
                return Ok(());
            }
            let full = sent == chunk;
            let data = Bytes::from(data);
            let sent = u64::try_from(sent).unwrap_or(u64::MAX);
            let write = ImportWriteParams {
                transfer_id,
                offset,
                data,
            };
            self.order(METHOD_IMPORT_WRITE, &write).await?;
            offset = offset.saturating_add(sent);
            if !full {
                return Ok(());
            }
        }
    }

    /// Packs `paths` on the guest agent and copies the stream into `into` a
    /// chunk at a time, until the guest agent says it ended.
    pub(super) async fn export_tree(
        &self,
        paths: &[&str],
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> Result<()> {
        let open = ExportOpenParams {
            paths: paths.iter().copied().map(Cow::Borrowed).collect(),
        };
        let TransferOpened { transfer_id } = self.fetch(METHOD_EXPORT_OPEN, &open).await?;
        self.read_export(transfer_id, into).await
    }

    /// Walks the workspace for a save's pin and copies the export of what
    /// changed into `into`.
    pub(super) async fn walk_pin(
        &self,
        parent: Option<&str>,
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> Result<PinWalked> {
        let walk = PinWalkParams {
            parent: parent.map(str::to_owned),
        };
        let walked: PinWalked = self.fetch(METHOD_PIN_WALK, &walk).await?;
        self.read_export(walked.transfer_id, into).await?;
        Ok(walked)
    }

    /// Copies export `transfer_id` into `into` a chunk at a time, until the
    /// guest agent says it ended.
    async fn read_export(
        &self,
        transfer_id: u64,
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> Result<()> {
        let mut offset = 0_u64;
        loop {
            let read = ExportReadParams {
                transfer_id,
                offset,
            };
            let ExportChunk { data, end } = self.fetch(METHOD_EXPORT_READ, &read).await?;
            if let Err(failure) = into.write_all(&data).await {
                if !end {
                    self.give_up(transfer_id).await;
                }
                return Err(failure.into());
            }
            if end {
                return Ok(into.flush().await?);
            }
            offset = offset.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX));
        }
    }

    /// Tells the guest agent to forget a transfer this client will not finish.
    /// Its answer changes nothing: the caller already has the failure that
    /// ended the transfer, and a closed link took the spool with it.
    async fn give_up(&self, transfer_id: u64) {
        let forget = TransferParams { transfer_id };
        let _forgotten = self.order(METHOD_TRANSFER_DROP, &forget).await;
    }
}
