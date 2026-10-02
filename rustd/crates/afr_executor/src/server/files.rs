//! File calls, confined to the workspace.
//!
//! Every call goes through one `cap_std` directory handle, which refuses a path
//! that leaves it — through `..`, through a symbolic link, or as an absolute
//! path — on every platform. The kernel sandbox already holds writes to the
//! workspace; this is the second wall, and the one a refusal is reported by.

use std::io::{self, Read as _};
use std::path::{Component, Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::error::{self, Error, Result};
use crate::protocol::{EntryWire, KindWire, ListResult, MAX_READ_BYTES, ReadResult, encode};

/// The workspace itself, named relative to itself.
const CURRENT_DIRECTORY: &str = ".";

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
    pub(super) fn directory(&self, path: Option<&str>) -> Result<PathBuf> {
        let inside = path.map_or(Ok(Path::new(CURRENT_DIRECTORY)), |path| self.inside(path))?;
        self.dir.open_dir(inside).map_err(confined)?;
        Ok(self.root.join(inside))
    }

    /// Reads a file, at most `max_bytes` of it.
    pub(super) fn read(&self, path: &str, max_bytes: u64) -> Result<ReadResult> {
        let limit = max_bytes.min(MAX_READ_BYTES);
        let mut content = Vec::new();
        self.dir
            .open(self.inside(path)?)
            .map_err(confined)?
            .take(limit.saturating_add(1))
            .read_to_end(&mut content)?;
        let kept = usize::try_from(limit).unwrap_or(usize::MAX);
        let truncated = content.len() > kept;
        content.truncate(kept);
        Ok(ReadResult {
            content: encode(&content),
            truncated,
        })
    }

    /// Writes a file, replacing what was there.
    pub(super) fn write(&self, path: &str, content: &[u8]) -> Result<()> {
        self.dir
            .write(self.inside(path)?, content)
            .map_err(confined)
    }

    /// Lists a directory.
    pub(super) fn list(&self, path: &str) -> Result<ListResult> {
        let entries = self
            .dir
            .read_dir(self.inside(path)?)
            .map_err(confined)?
            .map(|entry| {
                let entry = entry?;
                let kind = entry.file_type()?;
                Ok(EntryWire {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    kind: if kind.is_symlink() {
                        KindWire::Symlink
                    } else if kind.is_dir() {
                        KindWire::Directory
                    } else if kind.is_file() {
                        KindWire::File
                    } else {
                        KindWire::Other
                    },
                    size: entry.metadata()?.len(),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(ListResult { entries })
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

/// `cap_std` answers an escape as permission denied; the sandbox answers a
/// path it will not open the same way, and both are a refused path.
fn confined(failure: io::Error) -> Error {
    if failure.kind() == io::ErrorKind::PermissionDenied {
        error::path_refused()
    } else {
        failure.into()
    }
}

#[cfg(test)]
#[path = "files/tests.rs"]
mod tests;
