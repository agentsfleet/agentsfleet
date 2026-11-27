//! The wire of a tree crossing in chunks: an import opened, written and
//! committed, and an export opened and read to its end.
//!
//! A tree of any size crosses because no one message carries it: each chunk
//! rides its own frame, written at an offset the client names, so the
//! guest agent keeps no cursor and a chunk sent twice lands where it did.

use std::borrow::Cow;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use super::{MAX_FRAME_BYTES, base64_bytes};

/// `fs/import_open`: open a spool for a tar stream to unpack under a path.
pub const METHOD_IMPORT_OPEN: &str = "fs/import_open";
/// `fs/import_write`: write one chunk of the stream into its spool.
pub const METHOD_IMPORT_WRITE: &str = "fs/import_write";
/// `fs/import_commit`: check every entry, then unpack the stream.
pub const METHOD_IMPORT_COMMIT: &str = "fs/import_commit";
/// `fs/export_open`: pack workspace paths into a spool.
pub const METHOD_EXPORT_OPEN: &str = "fs/export_open";
/// `fs/export_read`: read one chunk of a packed stream.
pub const METHOD_EXPORT_READ: &str = "fs/export_read";
/// `fs/transfer_drop`: give up a transfer the client will not finish.
pub const METHOD_TRANSFER_DROP: &str = "fs/transfer_drop";

/// The most raw bytes one chunk carries in a frame of `frame` bytes.
///
/// Base64 writes four bytes for every three, and half the frame is left to
/// the envelope, so a chunk fits whatever frame a link's starter chose.
#[must_use]
pub const fn transfer_chunk_bytes(frame: usize) -> usize {
    let chunk = frame / 2 / 4 * 3;
    if chunk == 0 { 1 } else { chunk }
}

/// The chunk an unconfigured link carries, derived from the 64 MiB frame
/// ceiling ([`MAX_FRAME_BYTES`]) and base64's expansion: 24 MiB, 32 MiB once
/// encoded.
pub const TRANSFER_CHUNK_BYTES: usize = transfer_chunk_bytes(MAX_FRAME_BYTES);

/// `fs/import_open` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportOpenParams<'a> {
    /// Where the stream unpacks, inside the workspace; made when absent.
    pub at: Cow<'a, str>,
}

/// `fs/import_open` and `fs/export_open` result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransferOpened {
    /// The number later chunks name the transfer by.
    pub transfer_id: u64,
}

/// `fs/import_write` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportWriteParams {
    /// The transfer the chunk belongs to.
    pub transfer_id: u64,
    /// Where in the stream the chunk starts.
    pub offset: u64,
    /// The chunk.
    #[serde(with = "base64_bytes")]
    pub data: Bytes,
}

/// `fs/import_commit` and `fs/transfer_drop` parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransferParams {
    /// The transfer.
    pub transfer_id: u64,
}

/// `fs/export_open` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportOpenParams<'a> {
    /// The paths to pack, inside the workspace, each with what is below it.
    pub paths: Vec<Cow<'a, str>>,
}

/// `fs/export_read` parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExportReadParams {
    /// The transfer.
    pub transfer_id: u64,
    /// Where in the stream to read from.
    pub offset: u64,
}

/// `fs/export_read` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportChunk {
    /// The chunk.
    #[serde(with = "base64_bytes")]
    pub data: Bytes,
    /// Whether the stream ends with it; the guest agent has then forgotten the
    /// transfer.
    pub end: bool,
}
