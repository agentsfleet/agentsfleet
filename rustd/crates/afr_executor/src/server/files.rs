//! File calls, confined to the workspace.
//!
//! Every call goes through one `cap_std` directory handle, which refuses a path
//! that leaves it — through `..`, through a symbolic link, or as an absolute
//! path — on every platform. The kernel sandbox already holds writes to the
//! workspace; this is the second wall, and the one a refusal is reported by.
//!
//! Reads and writes open without blocking and then insist on a regular file:
//! a pipe opened for reading waits for a writer that may never come, and a
//! device is not the workspace's to hand out. Checking after the open, on the
//! handle, leaves no window to swap the file in between.

use std::io::{self, Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, OpenOptions, OpenOptionsExt as _};
use rustix::fs::OFlags;
use rustix::io::Errno;

use crate::api::{DirEntry, EntryKind, Listing};
use crate::error::{self, Error, Result};
use crate::protocol::{MAX_FRAME_BYTES, MAX_READ_BYTES, ReadResult};

/// The workspace itself, named relative to itself.
const CURRENT_DIRECTORY: &str = ".";
/// The most entries one listing carries.
pub(crate) const MAX_LIST_ENTRIES: usize = 4_096;
/// The longest one listed entry can serialize to: a 255-byte name of control
/// characters, each escaped to six, and the entry's other fields.
const MAX_ENTRY_JSON_BYTES: usize = 255 * 6 + 128;
// A full listing must fit the frame the client reads, or the answer would
// end the connection instead of arriving.
const _: () = assert!(MAX_LIST_ENTRIES * MAX_ENTRY_JSON_BYTES < MAX_FRAME_BYTES);
/// Open without waiting: a pipe with no writer must not hold the call.
const NONBLOCK: i32 = OFlags::NONBLOCK.bits().cast_signed();

/// The workspace: its path, for working directories, and a handle that cannot
/// see past it, for everything else.
#[derive(Debug)]
pub(super) struct Workspace {
    root: PathBuf,
    dir: Dir,
}

impl Workspace {
    /// Opens `root` as the workspace.
    pub(super) fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_owned(),
            dir: Dir::open_ambient_dir(root, ambient_authority())?,
        })
    }

    /// The host path of a directory inside the workspace, for a process to
    /// start in; the workspace itself when none is named.
    ///
    /// Checked through the handle, then named by path for the process to
    /// change into, so a process already inside the sandbox could swap the
    /// directory for a link between the two. The sandbox's own mounts and its
    /// Landlock rules are the wall that move would meet; changing directory
    /// through the open handle instead would take `unsafe` code in the child
    /// between fork and exec, which this crate does not carry.
    pub(super) fn directory(&self, path: Option<&str>) -> Result<PathBuf> {
        let inside = path.map_or(Ok(Path::new(CURRENT_DIRECTORY)), |path| self.inside(path))?;
        self.dir.open_dir(inside).map_err(confined)?;
        Ok(self.root.join(inside))
    }

    /// Reads a regular file, at most `max_bytes` of it.
    pub(super) fn read(&self, path: &str, max_bytes: u64) -> Result<ReadResult> {
        let limit = max_bytes.min(MAX_READ_BYTES);
        let mut content = Vec::new();
        self.regular(path, OpenOptions::new().read(true), Parents::Keep)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut content)?;
        let kept = usize::try_from(limit).unwrap_or(usize::MAX);
        let truncated = content.len() > kept;
        content.truncate(kept);
        Ok(ReadResult {
            content: content.into(),
            truncated,
        })
    }

    /// Writes a regular file, replacing what was there and making any missing
    /// parent directories, all inside the workspace.
    pub(super) fn write(&self, path: &str, content: &[u8]) -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        Ok(self
            .regular(path, &mut options, Parents::Make)?
            .write_all(content)?)
    }

    /// Adds to the end of a regular file, making it and any missing parent
    /// directories when absent, all inside the workspace.
    pub(super) fn append(&self, path: &str, content: &[u8]) -> Result<()> {
        let mut options = OpenOptions::new();
        options.append(true).create(true);
        Ok(self
            .regular(path, &mut options, Parents::Make)?
            .write_all(content)?)
    }

    /// Removes a regular file. What the name is, is read through the handle
    /// without following it, so a link is refused rather than unlinked and a
    /// directory is refused rather than emptied.
    pub(super) fn delete(&self, path: &str) -> Result<()> {
        let inside = self.inside(path)?;
        if !self
            .dir
            .symlink_metadata(inside)
            .map_err(confined)?
            .is_file()
        {
            return Err(error::not_a_file());
        }
        self.dir.remove_file(inside).map_err(confined)
    }

    /// Lists a directory, up to [`MAX_LIST_ENTRIES`] of it.
    pub(super) fn list(&self, path: &str) -> Result<Listing> {
        let mut entries = self.dir.read_dir(self.inside(path)?).map_err(confined)?;
        let listed = entries
            .by_ref()
            .take(MAX_LIST_ENTRIES)
            .map(|entry| entry.and_then(|entry| described(&entry)))
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Listing {
            entries: listed,
            truncated: entries.next().is_some(),
        })
    }

    /// Opens `path` without blocking and refuses anything but a regular file.
    ///
    /// With [`Parents::Make`], an open that fails because a directory is
    /// missing makes the directories and tries once more. The open is always
    /// tried first, so an escape is refused as one before anything is made.
    fn regular(&self, path: &str, options: &mut OpenOptions, parents: Parents) -> Result<File> {
        let inside = self.inside(path)?;
        options.custom_flags(NONBLOCK);
        let file = match (self.dir.open_with(inside, options), parents) {
            (Err(missing), Parents::Make) if missing.kind() == io::ErrorKind::NotFound => {
                if let Some(parent) = inside.parent() {
                    self.dir.create_dir_all(parent).map_err(confined)?;
                }
                self.dir.open_with(inside, options)
            }
            (opened, _either) => opened,
        }
        .map_err(confined)?;
        if file.metadata()?.is_file() {
            Ok(file)
        } else {
            Err(error::not_a_file())
        }
    }

    /// `path` relative to the workspace: an absolute path must name a place
    /// under it, and no path may climb out of it by name.
    fn inside<'path>(&self, path: &'path str) -> Result<&'path Path> {
        let path = Path::new(path);
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .map_err(|_outside| error::path_refused())?
        } else {
            path
        };
        if relative
            .components()
            .any(|part| part == Component::ParentDir)
        {
            return Err(error::path_refused());
        }
        Ok(if relative.as_os_str().is_empty() {
            Path::new(CURRENT_DIRECTORY)
        } else {
            relative
        })
    }
}

/// One listed entry, as the caller sees it.
fn described(entry: &cap_std::fs::DirEntry) -> io::Result<DirEntry> {
    let kind = entry.file_type()?;
    Ok(DirEntry {
        name: entry.file_name().to_string_lossy().into_owned(),
        kind: if kind.is_symlink() {
            EntryKind::Symlink
        } else if kind.is_dir() {
            EntryKind::Directory
        } else if kind.is_file() {
            EntryKind::File
        } else {
            EntryKind::Other
        },
        size: entry.metadata()?.len(),
    })
}

/// Whether opening a file may make its missing parent directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parents {
    /// Only what exists is opened: a read.
    Keep,
    /// Missing directories are made: a write.
    Make,
}

/// Sorts a failed open. `cap_std` reports an escape as a permission refusal
/// that no system call produced, which is how it is told from a real `EACCES`;
/// a pipe with no reader refuses a non-blocking open for writing with
/// `ENXIO`, and that is a file that is not a regular one; a name that is not
/// there is the caller's, and gets the code a handler names to the model.
fn confined(failure: io::Error) -> Error {
    match failure.raw_os_error() {
        None if failure.kind() == io::ErrorKind::PermissionDenied => error::path_refused(),
        Some(code) if code == Errno::NXIO.raw_os_error() => error::not_a_file(),
        _other if failure.kind() == io::ErrorKind::NotFound => error::not_found(failure),
        _other => failure.into(),
    }
}

#[cfg(test)]
#[path = "files/append_delete_tests.rs"]
mod append_delete_tests;
#[cfg(test)]
#[path = "files/tests.rs"]
mod tests;
