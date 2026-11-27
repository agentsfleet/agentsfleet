//! A tree packed into a tar stream, from paths inside a directory handle.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Seek as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use cap_std::fs::{Dir, OpenOptions, PermissionsExt as _};
use rustix::fs::SeekFrom;
use rustix::io::Errno;
use tar::{Builder, EntryType, Header};

use super::{CURRENT_DIRECTORY, PERMISSION_BITS, Parents, confined, regular};
use crate::error::Result;

/// Packs `paths` inside `dir`, and everything below each, into `into`.
///
/// `.` packs what `dir` holds. A link is packed as its target's name, never
/// followed, and children are packed in name order, so one tree always packs
/// to the same stream. A file with several names is packed under the first
/// and as a link to it under each other, and a file's holes are left out, so
/// the stream carries each byte the tree holds on disk once.
///
/// # Errors
/// A path that leaves `dir` or is not there, or a read or write that failed.
pub fn pack(dir: &Dir, paths: &[&Path], into: impl io::Write) -> Result<()> {
    let mut packing = Packing {
        builder: Builder::new(into),
        named: HashMap::new(),
    };
    for path in paths {
        if *path == Path::new(CURRENT_DIRECTORY) {
            packing.children(dir, Path::new(""))?;
        } else {
            packing.entry(dir, path)?;
        }
    }
    packing.builder.finish()?;
    Ok(())
}

/// One stream being packed.
struct Packing<W: io::Write> {
    builder: Builder<W>,
    /// Each packed file that has another name, by its device and inode, and
    /// the name it was packed under: what its other names link to.
    named: HashMap<(u64, u64), PathBuf>,
}

impl<W: io::Write> Packing<W> {
    /// Packs one entry, and what is below it when it is a directory.
    fn entry(&mut self, dir: &Dir, path: &Path) -> Result<()> {
        let found = dir.symlink_metadata(path).map_err(confined)?;
        let mut header = Header::new_gnu();
        header.set_size(0);
        header.set_mode(found.permissions().mode() & PERMISSION_BITS);
        if let Ok(modified) = found.modified()
            && let Ok(since) = modified.into_std().duration_since(std::time::UNIX_EPOCH)
        {
            header.set_mtime(since.as_secs());
        }
        let kind = found.file_type();
        if kind.is_symlink() {
            header.set_entry_type(EntryType::Symlink);
            let target = dir.read_link_contents(path).map_err(confined)?;
            self.builder.append_link(&mut header, path, target)?;
        } else if kind.is_dir() {
            header.set_entry_type(EntryType::Directory);
            self.builder.append_data(&mut header, path, io::empty())?;
            self.children(dir, path)?;
        } else if kind.is_file() {
            self.file(dir, path, header)?;
        } else {
            // A socket, a pipe or a device: nothing a tree carries.
        }
        Ok(())
    }

    /// Packs the regular file at `path`: its content, holes left out, or,
    /// when the stream holds it already under another name, a link to that
    /// name under `header`.
    fn file(&mut self, dir: &Dir, path: &Path, mut header: Header) -> Result<()> {
        let mut file = regular(dir, path, OpenOptions::new().read(true), Parents::Keep)?.into_std();
        // The opened file's own identity, whatever its name says by now.
        let found = file.metadata()?;
        let identity = (found.nlink() > 1).then(|| (found.dev(), found.ino()));
        if let Some(first) = identity.and_then(|identity| self.named.get(&identity)) {
            header.set_entry_type(EntryType::Link);
            self.builder.append_link(&mut header, path, first)?;
            return Ok(());
        }
        self.builder.sparse(holes_are_holes(&mut file, &found)?);
        self.builder.append_file(path, &mut file)?;
        if let Some(identity) = identity {
            self.named.insert(identity, path.to_owned());
        }
        Ok(())
    }

    /// Packs every entry of the directory `path`, in name order.
    fn children(&mut self, dir: &Dir, path: &Path) -> Result<()> {
        let listed = if path.as_os_str().is_empty() {
            Path::new(CURRENT_DIRECTORY)
        } else {
            path
        };
        let mut names = dir
            .read_dir(listed)
            .map_err(confined)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<io::Result<Vec<_>>>()?;
        names.sort();
        for name in names {
            self.entry(dir, &path.join(name))?;
        }
        Ok(())
    }
}

/// Whether the builder may read `file`'s unallocated ranges as holes.
///
/// It reads a file holding bytes but no block as one hole. A file system
/// that keeps a small file inside its inode, or has not allocated a new
/// file's blocks yet, reports no block for a file that has content, which
/// would pack as zeros; so a file with no block is packed as a hole only
/// when the kernel finds no data in it either.
fn holes_are_holes(file: &mut File, found: &std::fs::Metadata) -> io::Result<bool> {
    if found.blocks() > 0 {
        return Ok(true);
    }
    // `ENXIO`: no data at or after the start, so all of it is a hole.
    let data = rustix::fs::seek(&*file, SeekFrom::Data(0));
    file.rewind()?;
    Ok(matches!(data, Err(Errno::NXIO)))
}
