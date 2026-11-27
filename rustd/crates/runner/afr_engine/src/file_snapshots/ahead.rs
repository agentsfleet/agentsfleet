//! A saved tree's objects read a window ahead of the file that takes them.
//!
//! ```text
//!   objects, in manifest order   a1 a2 a3 b1 │ b2 c1 c2 c3 │ c4 …
//!   one window                   read together, at most READ_WINDOW in flight
//!   next()                       a1, a2, a3, b1, then the next window
//! ```
//!
//! A window spans files, so a tree of small files reads as many at once as
//! one large file does, and each file still takes its objects in order. The
//! next window is read only once every chunk of the last was taken, so a
//! restore holds one window of plaintext and no more.

use std::collections::VecDeque;
use std::io;

use af_chunk_store::{ChunkSource, READ_WINDOW, Resolved};
use af_wire::durable_task::CheckpointEntry;
use zeroize::Zeroizing;

use crate::error::Result;

/// Why a window's answer is not one a restore lands.
const UNEVEN_WINDOW: &str = "a window answered other than one chunk for each object";
const NO_OBJECT: &str = "a file names more objects than its tree does";

/// The objects of a tree's files, in manifest order.
type Objects<'a> = Box<dyn Iterator<Item = &'a String> + Send + 'a>;

/// A tree's objects, read from its source a window at a time.
pub(super) struct Ahead<'a> {
    source: &'a dyn ChunkSource,
    /// Every object no window has named yet.
    unread: Objects<'a>,
    /// What the last window read and no file has taken.
    read: VecDeque<Zeroizing<Vec<u8>>>,
}

impl<'a> Ahead<'a> {
    /// The objects `tree`'s files name, to be read from `source`.
    pub(super) fn new(tree: &'a Resolved, source: &'a dyn ChunkSource) -> Self {
        let files = tree.entries().values().filter_map(|entry| match entry {
            CheckpointEntry::File(file) => Some(&file.objects),
            CheckpointEntry::Directory { .. }
            | CheckpointEntry::Symlink { .. }
            | CheckpointEntry::Remove { .. } => None,
        });
        Self {
            source,
            unread: Box::new(files.flatten()),
            read: VecDeque::new(),
        }
    }

    /// The next object's plaintext, in manifest order.
    ///
    /// # Errors
    /// A read of its window failed, the window answered other than one chunk
    /// for each object, or the tree names no further object.
    pub(super) async fn next(&mut self) -> Result<Zeroizing<Vec<u8>>> {
        if self.read.is_empty() {
            let window: Vec<String> = self.unread.by_ref().take(READ_WINDOW).cloned().collect();
            let read = self.source.read_window(&window).await?;
            if read.len() != window.len() {
                return Err(invalid(UNEVEN_WINDOW));
            }
            self.read = read.into();
        }
        self.read.pop_front().ok_or_else(|| invalid(NO_OBJECT))
    }
}

/// A window no restore lands, saying why.
fn invalid(why: &'static str) -> crate::Error {
    io::Error::new(io::ErrorKind::InvalidData, why).into()
}
