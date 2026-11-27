//! What a pin's walk keeps of each entry: its kind and the stat fields that
//! move when its content or its metadata does, and the tick the walk began on.
//!
//! The fields are git's index identity: device, inode, size, mode and the
//! nanosecond modification and change times. Each is read through the
//! directory's descriptor with no file opened.

use std::ffi::OsStr;
use std::io;

use cap_std::fs::Dir;
use rustix::fs::{AtFlags, FileType, Mode, OFlags};

/// A clock reading, seconds and nanoseconds since the epoch.
pub(crate) type Tick = (i64, i64);

/// What an entry is; anything else the tree holds is never saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Directory,
    Symlink,
}

/// The stat fields a change to an entry moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) device: u64,
    pub(crate) inode: u64,
    pub(crate) size: u64,
    pub(crate) mode: u32,
    pub(crate) modified: Tick,
    pub(crate) changed: Tick,
}

/// One entry as a walk found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) kind: Kind,
    pub(crate) identity: Identity,
}

/// The clock file timestamps are stamped from, read now.
///
/// Linux stamps a file's times from the coarse realtime clock, which lags the
/// fine one by up to a tick; reading the fine one here could name a moment
/// after a write the coarse clock still dates before it, and the racy-clean
/// guard would miss that write.
#[must_use]
pub(crate) fn tick() -> Tick {
    #[cfg(target_os = "linux")]
    let clock = rustix::time::ClockId::RealtimeCoarse;
    #[cfg(not(target_os = "linux"))]
    let clock = rustix::time::ClockId::Realtime;
    let now = rustix::time::clock_gettime(clock);
    (now.tv_sec, now.tv_nsec)
}

/// The directory `name` inside `dir`, opened without following a link: the
/// walk found it a directory, and a tenant held still cannot swap it.
pub(crate) fn directory_at(dir: &Dir, name: &OsStr) -> io::Result<Dir> {
    let opened = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(Dir::from_std_file(std::fs::File::from(opened)))
}

/// `name` inside `dir`, never followed; none when it is a socket, a pipe or a
/// device.
pub(crate) fn seen_at(dir: &Dir, name: &OsStr) -> io::Result<Option<Seen>> {
    let (raw_mode, identity) = stat_at(dir, name)?;
    let kind = match FileType::from_raw_mode(raw_mode) {
        FileType::RegularFile => Kind::File,
        FileType::Directory => Kind::Directory,
        FileType::Symlink => Kind::Symlink,
        _special => return Ok(None),
    };
    Ok(Some(Seen { kind, identity }))
}

/// `statx`, which asks for the basic fields alone and opens nothing.
#[cfg(target_os = "linux")]
fn stat_at(dir: &Dir, name: &OsStr) -> io::Result<(rustix::fs::RawMode, Identity)> {
    let found = rustix::fs::statx(
        dir,
        name,
        AtFlags::SYMLINK_NOFOLLOW,
        rustix::fs::StatxFlags::BASIC_STATS,
    )?;
    let mode = u32::from(found.stx_mode);
    Ok((
        mode,
        Identity {
            device: (u64::from(found.stx_dev_major) << 32) | u64::from(found.stx_dev_minor),
            inode: found.stx_ino,
            size: found.stx_size,
            mode,
            modified: (found.stx_mtime.tv_sec, i64::from(found.stx_mtime.tv_nsec)),
            changed: (found.stx_ctime.tv_sec, i64::from(found.stx_ctime.tv_nsec)),
        },
    ))
}

/// `fstatat`, where `statx` is Linux's alone.
#[cfg(not(target_os = "linux"))]
fn stat_at(dir: &Dir, name: &OsStr) -> io::Result<(rustix::fs::RawMode, Identity)> {
    let found = rustix::fs::statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
    Ok((
        found.st_mode,
        Identity {
            device: u64::from(found.st_dev.cast_unsigned()),
            inode: found.st_ino,
            size: u64::try_from(found.st_size).unwrap_or_default(),
            mode: u32::from(found.st_mode),
            modified: (found.st_mtime, found.st_mtime_nsec),
            changed: (found.st_ctime, found.st_ctime_nsec),
        },
    ))
}
