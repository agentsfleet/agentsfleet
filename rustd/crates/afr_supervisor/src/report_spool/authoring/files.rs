//! All private report categories share one cleanup boundary and restrictive permissions.

use crate::{Result, error};
use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::Path,
};
use tempfile::NamedTempFile;

pub(super) const ACCOUNTING: &str = "accounting.json";
pub(super) const META: &str = "metadata.json";
pub(super) const CONTENT: &str = "content.enc";
pub(super) const KEY: &str = "content.key";
pub(super) const CLOCK: &str = "clock.json";

pub(super) fn directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error::config(
            "the private report directory is not a directory",
        ));
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| error::config("report path has no parent"))?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub(super) fn read(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > maximum {
        return Err(error::config(
            "a private report component is not a bounded regular file",
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(error::config(
            "a private report component grew beyond its bound",
        ));
    }
    Ok(bytes)
}

pub(super) fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)?;
        }
        Ok(_) => fs::remove_file(path)?,
        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(failure) => return Err(failure.into()),
    }
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

pub(super) fn clear_private(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name() != ACCOUNTING {
            remove(&entry.path())?;
        }
    }
    Ok(())
}

pub(super) fn clear_unknown(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if ![ACCOUNTING, META, CONTENT, KEY]
            .iter()
            .any(|name| entry.file_name() == *name)
        {
            remove(&entry.path())?;
        }
    }
    Ok(())
}
