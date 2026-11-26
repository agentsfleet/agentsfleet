//! What the executor's file calls answer with.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

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
