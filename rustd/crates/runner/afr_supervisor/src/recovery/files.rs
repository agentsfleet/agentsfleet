//! Frozen workspace access uses directory descriptors and never traverses links.

use crate::{Result, error};
use cap_std::fs::Dir;
use rustix::fs::{Mode, OFlags};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    os::unix::fs::MetadataExt as _,
    path::{Component, Path},
};

pub(super) const CHUNK: u64 = 8 * 1024 * 1024;
const MAX_BYTES: u64 = 24 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

/// Metadata pinned while the sandbox is frozen, verified again on the opened file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Identity {
    fn of(metadata: &std::fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum Entry {
    File {
        identity: Identity,
        executable: bool,
    },
    Directory,
    Symlink {
        target: String,
    },
}

/// An opened root is owned by the capture/restore operation, never a tenant path string.
#[derive(Debug)]
pub(super) struct Workspace {
    root: Dir,
}

impl Workspace {
    pub(super) fn require_empty(&self) -> Result<()> {
        if self.root.entries()?.next().is_some() {
            return Err(error::config(
                "checkpoint restore requires a fresh empty workspace",
            ));
        }
        Ok(())
    }

    pub(super) fn make_directory(&self, path: &str, owner: (u32, u32)) -> Result<()> {
        valid_path(path)?;
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let parent = self.directory(parent)?;
        rustix::fs::mkdirat(&parent, name, Mode::from_raw_mode(0o700))
            .map_err(std::io::Error::from)?;
        let directory = self.directory(path)?;
        owned(&directory, owner)?;
        rustix::fs::fchmod(&directory, Mode::from_raw_mode(0o700)).map_err(std::io::Error::from)?;
        Ok(())
    }

    pub(super) fn make_link(&self, path: &str, target: &str, owner: (u32, u32)) -> Result<()> {
        valid_path(path)?;
        valid_link(path, target)?;
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let parent = self.directory(parent)?;
        rustix::fs::symlinkat(target, &parent, name).map_err(std::io::Error::from)?;
        rustix::fs::chownat(
            &parent,
            name,
            Some(rustix::fs::Uid::from_raw(owner.0)),
            Some(rustix::fs::Gid::from_raw(owner.1)),
            rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(std::io::Error::from)?;
        Ok(())
    }

    pub(super) fn make_file(
        &self,
        path: &str,
        size: u64,
        executable: bool,
        owner: (u32, u32),
    ) -> Result<Restoring> {
        valid_path(path)?;
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let parent = self.directory(parent)?;
        let mode = Mode::from_raw_mode(if executable { 0o700 } else { 0o600 });
        let fd = rustix::fs::openat(
            &parent,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            mode,
        )
        .map_err(std::io::Error::from)?;
        let file = File::from(fd);
        owned(&file, owner)?;
        rustix::fs::fchmod(&file, mode).map_err(std::io::Error::from)?;
        Ok(Restoring {
            file,
            remaining: size,
        })
    }

    pub(super) fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        Ok(Self {
            root: Dir::from_std_file(File::from(fd)),
        })
    }

    /// Complete bounded inventory. Unsupported names and special files refuse capture.
    pub(super) fn inventory(
        &self,
        stop: &tokio_util::sync::CancellationToken,
    ) -> Result<BTreeMap<String, Entry>> {
        let mut entries = BTreeMap::new();
        let mut pending = vec![String::new()];
        let mut total = 0_u64;
        let mut names = 0_usize;
        while let Some(path) = pending.pop() {
            let directory = self.directory(&path)?;
            for entry in directory.entries()? {
                if stop.is_cancelled() {
                    return Err(error::config("checkpoint inventory cancelled"));
                }
                let entry = entry?;
                let name = entry
                    .file_name()
                    .to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| error::config("checkpoint path is not UTF-8"))?;
                let relative = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{path}/{name}")
                };
                valid_path(&relative)?;
                names = names
                    .checked_add(relative.len() + 128)
                    .ok_or_else(|| error::config("checkpoint path budget overflow"))?;
                if names > 64 * 1024 * 1024 {
                    return Err(error::config("checkpoint path budget exceeded"));
                }
                if entries.len() >= MAX_ENTRIES {
                    return Err(error::config("checkpoint entry allowance exceeded"));
                }
                let metadata = directory.symlink_metadata(&name)?;
                let kind = if metadata.is_dir() {
                    pending.push(relative.clone());
                    Entry::Directory
                } else if metadata.is_symlink() {
                    let target = directory
                        .read_link(&name)?
                        .into_os_string()
                        .to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| error::config("checkpoint link is not UTF-8"))?;
                    valid_link(&relative, &target)?;
                    names = names
                        .checked_add(target.len())
                        .ok_or_else(|| error::config("checkpoint link budget overflow"))?;
                    if names > 64 * 1024 * 1024 {
                        return Err(error::config("checkpoint path budget exceeded"));
                    }
                    Entry::Symlink { target }
                } else if metadata.is_file() {
                    let file = regular(&directory, &name)?;
                    let stat = file.metadata()?;
                    total = total
                        .checked_add(stat.len())
                        .ok_or_else(|| error::config("checkpoint size overflow"))?;
                    if total > MAX_BYTES {
                        return Err(error::config("checkpoint expanded allowance exceeded"));
                    }
                    Entry::File {
                        identity: Identity::of(&stat),
                        executable: stat.mode() & 0o111 != 0,
                    }
                } else {
                    return Err(error::config("checkpoint contains a special file"));
                };
                if entries.insert(relative, kind).is_some() {
                    return Err(error::config("checkpoint path repeated"));
                }
            }
        }
        super::links::validate(entries.iter().filter_map(|(path, entry)| match entry {
            Entry::Symlink { target } => Some((path.as_str(), target.as_str())),
            Entry::File { .. } | Entry::Directory => None,
        }))?;
        Ok(entries)
    }

    pub(super) fn chunk(
        &self,
        path: &str,
        identity: &Identity,
        offset: u64,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        valid_path(path)?;
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut file = regular(&self.directory(parent)?, name)?;
        if Identity::of(&file.metadata()?) != *identity || offset > identity.size {
            return Err(error::config("checkpoint file changed after inventory"));
        }
        let length = (identity.size - offset).min(CHUNK);
        let mut bytes = zeroize::Zeroizing::new(vec![
            0;
            usize::try_from(length).map_err(|source| {
                error::boundary("checkpoint chunk size overflow", source)
            })?
        ]);
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut bytes)?;
        if Identity::of(&file.metadata()?) != *identity {
            return Err(error::config("checkpoint file changed during capture"));
        }
        Ok(bytes)
    }

    fn directory(&self, path: &str) -> Result<Dir> {
        let mut directory = self.root.try_clone()?;
        if path.is_empty() {
            return Ok(directory);
        }
        valid_path(path)?;
        for part in path.split('/') {
            let fd = rustix::fs::openat(
                &directory,
                part,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?;
            directory = Dir::from_std_file(File::from(fd));
        }
        Ok(directory)
    }
}

#[derive(Debug)]
pub(super) struct Restoring {
    file: File,
    remaining: u64,
}

impl Restoring {
    pub(super) fn append(&mut self, bytes: &[u8]) -> Result<()> {
        if crate::byte_count(bytes.len()) > self.remaining {
            return Err(error::config("checkpoint file exceeds declared size"));
        }
        self.file.write_all(bytes)?;
        self.remaining -= crate::byte_count(bytes.len());
        Ok(())
    }

    pub(super) fn finish(self) -> Result<()> {
        if self.remaining != 0 {
            return Err(error::config("checkpoint file is incomplete"));
        }
        self.file.sync_all()?;
        Ok(())
    }
}

fn owned(file: &impl std::os::fd::AsFd, owner: (u32, u32)) -> Result<()> {
    rustix::fs::fchown(
        file,
        Some(rustix::fs::Uid::from_raw(owner.0)),
        Some(rustix::fs::Gid::from_raw(owner.1)),
    )
    .map_err(std::io::Error::from)?;
    Ok(())
}

pub(super) const fn size(entry: &Entry) -> u64 {
    match entry {
        Entry::File { identity, .. } => identity.size,
        Entry::Directory | Entry::Symlink { .. } => 0,
    }
}

fn regular(directory: &Dir, name: &str) -> Result<File> {
    let fd = rustix::fs::openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(error::config("checkpoint content is not a regular file"));
    }
    Ok(file)
}

fn valid_path(path: &str) -> Result<()> {
    garde::Validate::validate(&PathText { value: path })?;
    if path.split('/').any(|part| matches!(part, "" | "." | "..")) {
        return Err(error::config("checkpoint path is not a safe relative path"));
    }
    Ok(())
}

pub(super) fn valid_link(path: &str, target: &str) -> Result<()> {
    garde::Validate::validate(&PathText { value: target })?;
    let mut depth = path.split('/').count() - 1;
    for part in Path::new(target).components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(error::config("checkpoint symlink escapes its workspace"));
            }
        }
    }
    Ok(())
}

#[derive(garde::Validate)]
struct PathText<'a> {
    #[garde(length(bytes, min = 1, max = 4096), custom(path_characters))]
    value: &'a str,
}

fn path_characters<C>(value: &str, _context: &C) -> garde::Result {
    if value.contains('\\') || value.chars().any(char::is_control) {
        Err(garde::Error::new(
            "checkpoint paths cannot contain backslashes or control characters",
        ))
    } else {
        Ok(())
    }
}
