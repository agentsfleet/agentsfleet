//! What the guest agent's file calls answer with.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::protocol::MAX_FRAME_BYTES;

/// The most entries one listing carries.
/// Kept: a listing past one frame would fail the whole call.
pub const MAX_LIST_ENTRIES: usize = 4_096;
/// The longest one listed entry can serialize to: a 255-byte name of control
/// characters, each escaped to six, and the entry's other fields.
const MAX_ENTRY_JSON_BYTES: usize = 255 * 6 + 128;
/// The longest a full listing can serialize to. It must fit the frame the
/// client reads, or the answer would end the connection instead of arriving:
/// a starter choosing a frame keeps it longer than this.
pub const MAX_LISTING_BYTES: usize = MAX_LIST_ENTRIES * MAX_ENTRY_JSON_BYTES;
const _: () = assert!(MAX_LISTING_BYTES < MAX_FRAME_BYTES);

/// What a directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link, not followed.
    Symlink,
    /// Anything else: a socket, a device, a pipe.
    Other,
}

/// One entry of a listed directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    /// The entry's name, without its directory.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// Its size in bytes, as the file system reports it.
    pub size: u64,
}

/// A directory's entries, up to the most one answer carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The entries, in no particular order.
    pub entries: Vec<DirEntry>,
    /// Whether the directory held more than were listed.
    pub truncated: bool,
}

/// What one import unpacked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    /// The entries written: files, directories and links.
    pub entries: u64,
    /// The bytes of file content written.
    pub bytes: u64,
}

/// A file's bytes, up to the limit the caller asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    /// The bytes read.
    pub data: Bytes,
    /// Whether the file held more than was read.
    pub truncated: bool,
}
