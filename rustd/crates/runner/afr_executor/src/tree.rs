//! A tree as a tar stream: packed from paths inside a directory handle, or
//! checked whole and unpacked under one, every entry through the handle.
//!
//! The executor's `import` and `export` serve these over its workspace, and
//! the supervisor runs the same two over its own staging directory on the
//! host, so a stream is held to one set of checks on both ends of the link.
//!
//! An unpack reads its stream from a spool, a file it can read twice, so the
//! stream is checked whole before its first entry is written: one entry
//! leaving the directory refuses all of it. Ownership in the stream is
//! ignored, so what lands is the unpacking user's; the mode's permission bits
//! are kept and its set-id bits are not.

use std::fs::File;
use std::io::{self, Seek as _, SeekFrom};
use std::path::{Component, Path, PathBuf};

use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt as _, Permissions, PermissionsExt as _};
use rustix::fs::OFlags;
use rustix::io::Errno;
use tar::{Archive, Builder, Entry, EntryType, Header};

use crate::api::Imported;
use crate::error::{self, Error, Result};

/// A directory named relative to itself.
pub(crate) const CURRENT_DIRECTORY: &str = ".";
/// The permission bits an entry keeps: never set-user-id, set-group-id or
/// sticky, which a stream from elsewhere has no business granting.
const PERMISSION_BITS: u32 = 0o777;
/// Open without waiting: a pipe with no writer must not hold the call.
const NONBLOCK: i32 = OFlags::NONBLOCK.bits().cast_signed();

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

/// Packs `paths` inside `dir`, and everything below each, into `into`.
///
/// `.` packs what `dir` holds. A link is packed as its target's name, never
/// followed, and children are packed in name order, so one tree always packs
/// to the same stream.
///
/// # Errors
/// A path that leaves `dir` or is not there, or a read or write that failed.
pub fn pack(dir: &Dir, paths: &[&Path], into: impl io::Write) -> Result<()> {
    let mut builder = Builder::new(into);
    for path in paths {
        if *path == Path::new(CURRENT_DIRECTORY) {
            pack_children(dir, &mut builder, Path::new(""))?;
        } else {
            pack_entry(dir, &mut builder, path)?;
        }
    }
    builder.finish()?;
    Ok(())
}

/// Opens `inside` without blocking and refuses anything but a regular file.
///
/// With [`Parents::Make`], an open that fails because a directory is missing
/// makes the directories and tries once more. The open is always tried
/// first, so an escape is refused as one before anything is made.
pub(crate) fn regular(
    dir: &Dir,
    inside: &Path,
    options: &mut OpenOptions,
    parents: Parents,
) -> Result<cap_std::fs::File> {
    options.custom_flags(NONBLOCK);
    let file = match (dir.open_with(inside, options), parents) {
        (Err(missing), Parents::Make) if missing.kind() == io::ErrorKind::NotFound => {
            // `inside` is never empty (the directory itself reads as `.`),
            // so it always has a parent to make.
            inside
                .parent()
                .map_or(Ok(()), |parent| dir.create_dir_all(parent))
                .map_err(confined)?;
            dir.open_with(inside, options)
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

/// Whether opening a file may make its missing parent directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Parents {
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
pub(crate) fn confined(failure: io::Error) -> Error {
    match failure.raw_os_error() {
        None if failure.kind() == io::ErrorKind::PermissionDenied => error::path_refused(),
        Some(code) if code == Errno::NXIO.raw_os_error() => error::not_a_file(),
        _other if failure.kind() == io::ErrorKind::NotFound => error::not_found(failure),
        _other => failure.into(),
    }
}

/// Writes one file's content and gives it `mode`.
fn unpack_file(dir: &Dir, path: &Path, mode: u32, mut entry: Entry<'_, &File>) -> Result<()> {
    clear(dir, path)?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = regular(dir, path, &mut options, Parents::Make)?;
    io::copy(&mut entry, &mut file)?;
    Ok(file.set_permissions(Permissions::from_mode(mode))?)
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

/// Packs one entry, and what is below it when it is a directory.
fn pack_entry<W: io::Write>(dir: &Dir, builder: &mut Builder<W>, path: &Path) -> Result<()> {
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
        builder.append_link(&mut header, path, target)?;
    } else if kind.is_dir() {
        header.set_entry_type(EntryType::Directory);
        builder.append_data(&mut header, path, io::empty())?;
        pack_children(dir, builder, path)?;
    } else if kind.is_file() {
        let file = regular(dir, path, OpenOptions::new().read(true), Parents::Keep)?;
        header.set_entry_type(EntryType::Regular);
        header.set_size(file.metadata()?.len());
        builder.append_data(&mut header, path, file)?;
    } else {
        // A socket, a pipe or a device: nothing a tree carries.
    }
    Ok(())
}

/// Packs every entry of the directory `path`, in name order.
fn pack_children<W: io::Write>(dir: &Dir, builder: &mut Builder<W>, path: &Path) -> Result<()> {
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
        pack_entry(dir, builder, &path.join(name))?;
    }
    Ok(())
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
