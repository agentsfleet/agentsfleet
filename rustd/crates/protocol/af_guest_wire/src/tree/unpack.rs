//! A tar stream checked whole, then unpacked under a directory handle.

use std::fs::File;
use std::io::{self, Read, Seek as _, SeekFrom, Write as _};
use std::path::{Component, Path, PathBuf};

use cap_std::fs::{Dir, OpenOptions, Permissions, PermissionsExt as _};
use tar::{Archive, Entry, EntryType};

use super::{PERMISSION_BITS, Parents, confined, regular};
use crate::Imported;
use crate::error::{self, Result};

/// How much of an entry is read at a time while it is written.
const COPY_BYTES: usize = 64 * 1024;

/// One checked entry: where it lands, and what it is.
enum Planned {
    Directory,
    File,
    Symlink(PathBuf),
    Hardlink(PathBuf),
    /// Nothing to write: metadata for the whole stream, such as `git
    /// archive`'s commit header, or the unpack's root itself.
    Skipped,
}

/// Checks every entry of the tar stream in `spool`, read from its start,
/// then unpacks it under `at` inside `dir`, made when absent; what it wrote.
///
/// # Errors
/// An entry whose name or link target leaves `at`'s directory, a device or
/// a pipe, a stream that is not tar, or a write that failed. A refusal found
/// by the check writes nothing.
pub fn unpack(dir: &Dir, at: &Path, spool: &File) -> Result<Imported> {
    let imported = checked(at, spool)?;
    let mut rewind = spool;
    rewind.seek(SeekFrom::Start(0))?;
    dir.create_dir_all(at).map_err(confined)?;
    let mut directories = Vec::new();
    for entry in Archive::new(spool).entries()? {
        let entry = entry?;
        let plan = planned(at, &entry)?;
        let path = at.join(normalized(&entry.path()?)?);
        let mode = entry.header().mode()? & PERMISSION_BITS;
        match plan {
            Planned::Directory => {
                dir.create_dir_all(&path).map_err(confined)?;
                directories.push((path, mode));
            }
            Planned::File => unpack_file(dir, &path, mode, entry)?,
            Planned::Symlink(target) => {
                clear(dir, &path)?;
                dir.symlink(target, &path).map_err(confined)?;
            }
            Planned::Hardlink(source) => {
                clear(dir, &path)?;
                dir.hard_link(source, dir, &path).map_err(confined)?;
            }
            Planned::Skipped => {}
        }
    }
    // Last, deepest first: a directory without write permission would
    // refuse the entries below it.
    for (path, mode) in directories.into_iter().rev() {
        dir.set_permissions(&path, Permissions::from_mode(mode))
            .map_err(confined)?;
    }
    Ok(imported)
}

/// Writes one file's content and gives it `mode`.
fn unpack_file(dir: &Dir, path: &Path, mode: u32, mut entry: Entry<'_, &File>) -> Result<()> {
    clear(dir, path)?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = regular(dir, path, &mut options, Parents::Make)?;
    copy_keeping_holes(&mut entry, &mut file)?;
    Ok(file.set_permissions(Permissions::from_mode(mode))?)
}

/// Copies `from` into the empty `file`, stepping over each run of zeros
/// instead of writing it.
///
/// The stream leaves a file's holes out and the reader hands them back as
/// zeros, one hole or one run of data per read. Written, those zeros would
/// take the blocks the hole never did, and a file a few bytes long in its
/// stream would land as large as it claims to be; stepped over, it lands in
/// the space its stream took.
fn copy_keeping_holes(from: &mut impl Read, file: &mut cap_std::fs::File) -> io::Result<()> {
    let mut block = vec![0; COPY_BYTES];
    let mut length = 0_u64;
    loop {
        let read = from.read(&mut block)?;
        let Some(bytes) = block.get(..read).filter(|bytes| !bytes.is_empty()) else {
            // A file that ends in a hole is as long as the hole reaches.
            return file.set_len(length);
        };
        length = length.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if bytes.iter().all(|byte| *byte == 0) {
            file.seek(SeekFrom::Start(length))?;
        } else {
            file.write_all(bytes)?;
        }
    }
}

/// Removes a link or file already at `path`, so what lands replaces it
/// rather than writing through it.
fn clear(dir: &Dir, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        dir.create_dir_all(parent).map_err(confined)?;
    }
    match dir.symlink_metadata(path) {
        Ok(found) if !found.is_dir() => dir.remove_file(path).map_err(confined),
        Ok(_directory) => Ok(()),
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(failure) => Err(confined(failure)),
    }
}

/// Every entry of the stream in `spool` checked, read from its start, and
/// what unpacking it writes.
fn checked(at: &Path, spool: &File) -> Result<Imported> {
    let mut rewind = spool;
    rewind.seek(SeekFrom::Start(0))?;
    let mut imported = Imported::default();
    for entry in Archive::new(spool).entries()? {
        let entry = entry?;
        match planned(at, &entry)? {
            Planned::Skipped => {}
            Planned::File => {
                imported.entries += 1;
                imported.bytes += entry.size();
            }
            Planned::Directory | Planned::Symlink(_) | Planned::Hardlink(_) => {
                imported.entries += 1;
            }
        }
    }
    Ok(imported)
}

/// What `entry` is, refused when its name or its link's target leaves the
/// directory: an absolute name or one with `..`, a symbolic link to an
/// absolute target or one that resolves, from the link's own directory,
/// above the directory, and a hard link naming anything but another entry of
/// the stream. A device or a pipe is refused as no regular file.
fn planned(at: &Path, entry: &Entry<'_, &File>) -> Result<Planned> {
    let kind = entry.header().entry_type();
    if kind.is_pax_global_extensions() {
        return Ok(Planned::Skipped);
    }
    let path = normalized(&entry.path()?)?;
    let link = || entry.link_name()?.ok_or_else(error::path_refused);
    Ok(match kind {
        // The unpack's root itself, made already.
        EntryType::Directory if path.as_os_str().is_empty() => Planned::Skipped,
        EntryType::Directory => Planned::Directory,
        _ if path.as_os_str().is_empty() => return Err(error::path_refused()),
        EntryType::Regular | EntryType::Continuous | EntryType::GNUSparse => Planned::File,
        EntryType::Symlink => {
            let target = link()?.into_owned();
            let from = at.join(&path);
            if target.is_absolute() || !resolves_inside(from.parent(), &target) {
                return Err(error::path_refused());
            }
            Planned::Symlink(target)
        }
        EntryType::Link => Planned::Hardlink(at.join(normalized(&link()?)?)),
        _other => return Err(error::not_a_file()),
    })
}

/// A name inside the stream, relative and with no `..`: what it names under
/// the unpack's root. `.` parts are dropped.
fn normalized(path: &Path) -> Result<PathBuf> {
    path.components()
        .try_fold(PathBuf::new(), |mut kept, part| match part {
            Component::Normal(name) => {
                kept.push(name);
                Ok(kept)
            }
            Component::CurDir => Ok(kept),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                Err(error::path_refused())
            }
        })
}

/// Whether `target`, read from the directory `from`, stays inside the
/// unpack's directory, by its names alone.
fn resolves_inside(from: Option<&Path>, target: &Path) -> bool {
    let mut depth = from.map_or(0, |from| {
        from.components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count()
    });
    for part in target.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}
