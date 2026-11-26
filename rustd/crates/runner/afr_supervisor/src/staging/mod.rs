//! Where a sandbox's files wait on the host on their way in or out.
//!
//! A repository is checked out here before it is imported, a workspace is
//! exported here before a checkpoint reads it, and a checkpoint is restored
//! here before it is imported.
//!
//! ```text
//!   <home>/staging/<sandbox>-<random>/  one step's tree: 0700, the runner's,
//!                                       removed when the step ends
//!   <home>/staging/                     emptied at boot of what a crash left
//! ```
//!
//! The workspace itself stays the engine's: files cross only through the
//! executor's `import` and `export`, and both ends check the stream with the
//! same [`afr_executor::tree`] functions. A tree crosses through an anonymous
//! spool beside the stage, never inside it, so no spool is packed or
//! unpacked over.

use std::fs::{self, File};
use std::io::{self, Seek as _, SeekFrom};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use af_core::id::Uuid7;
use afr_executor::{Executor, Imported};
use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::error::{self, Result};

/// Only the runner reads or writes a stage: a tenant's files pass through it.
const STAGE_MODE: u32 = 0o700;
/// The event a stage the boot sweep could not remove is logged under.
const EVENT_SWEEP_LEFT: &str = "staging_sweep_left";
/// The workspace, and a stage, named relative to itself.
const WHOLE: &str = ".";
/// Why a stage could not be filled from the workspace.
const DETAIL_EXPORT: &str = "the workspace would not export into its stage";
/// Why a stage could not be imported into the workspace.
const DETAIL_IMPORT: &str = "the workspace refused a staged tree";
/// Why a restore found the workspace already holding files.
const DETAIL_NOT_EMPTY: &str = "checkpoint restore requires a fresh empty workspace";

/// The directory every stage is made under.
#[derive(Debug, Clone)]
pub struct Staging {
    root: PathBuf,
}

impl Staging {
    /// Stages made under `root`, which exists and only the runner reaches.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Removes whatever is under `root` and makes it again, empty: what a
    /// runner that crashed mid-step left behind.
    ///
    /// # Errors
    /// Something left there could not be removed, or the directory made.
    pub fn sweep(root: &Path) -> Result<Self> {
        match fs::remove_dir_all(root) {
            Ok(()) => {}
            Err(missing) if missing.kind() == io::ErrorKind::NotFound => {}
            // A stage a tenant made unremovable is a leftover, never a
            // reason to stay down: each new stage is a fresh name beside it.
            Err(failure) => {
                let reason = failure.to_string();
                let event = EVENT_SWEEP_LEFT;
                tracing::warn!(reason, event);
            }
        }
        fs::create_dir_all(root)?;
        fs::set_permissions(root, fs::Permissions::from_mode(STAGE_MODE))?;
        Ok(Self::new(root.to_owned()))
    }

    /// A fresh, empty stage for one step of `sandbox`, removed when dropped.
    ///
    /// # Errors
    /// The directory could not be made.
    pub fn stage(&self, sandbox: &Uuid7) -> Result<Staged> {
        let dir = tempfile::Builder::new()
            .prefix(&format!("{sandbox}-"))
            .permissions(fs::Permissions::from_mode(STAGE_MODE))
            .tempdir_in(&self.root)?;
        Ok(Staged {
            spools: self.root.clone(),
            dir,
        })
    }
}

/// One step's stage on the host.
#[derive(Debug)]
pub struct Staged {
    /// Where its spools are made: beside it, never in it.
    spools: PathBuf,
    dir: tempfile::TempDir,
}

impl Staged {
    /// The stage's path on the host.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The user and group a file restored into the stage is written as: the
    /// runner's own, since the import hands every file to the sandbox's user.
    #[must_use]
    pub fn owner() -> (u32, u32) {
        (
            rustix::process::getuid().as_raw(),
            rustix::process::getgid().as_raw(),
        )
    }

    /// Packs what the stage holds under `at` and imports it at `at` in the
    /// workspace `executor` serves; `.` is the whole stage and workspace.
    ///
    /// # Errors
    /// The stage would not pack, or the executor refused the stream.
    pub async fn import(&self, executor: &dyn Executor, at: &str) -> Result<Imported> {
        let (source, spool) = (self.path().join(at), self.spool()?);
        let packed = tokio::task::spawn_blocking(move || {
            let dir = Dir::open_ambient_dir(&source, ambient_authority())?;
            afr_executor::tree::pack(&dir, &[Path::new(WHOLE)], &spool)?;
            let mut rewind = &spool;
            rewind.seek(SeekFrom::Start(0))?;
            Ok::<_, afr_executor::Error>(spool)
        })
        .await?
        .map_err(|source| error::boundary(DETAIL_IMPORT, source))?;
        let mut stream = tokio::fs::File::from_std(packed);
        executor
            .import(at, &mut stream)
            .await
            .map_err(|source| error::boundary(DETAIL_IMPORT, source))
    }

    /// Exports the whole workspace `executor` serves and unpacks it into the
    /// stage, every entry checked as an import checks it.
    ///
    /// # Errors
    /// The executor would not export, or an entry would leave the stage.
    pub async fn export(&self, executor: &dyn Executor) -> Result<()> {
        let mut stream = tokio::fs::File::from_std(self.spool()?);
        executor
            .export(&[WHOLE], &mut stream)
            .await
            .map_err(|source| error::boundary(DETAIL_EXPORT, source))?;
        let (spool, into) = (stream.into_std().await, self.path().to_owned());
        tokio::task::spawn_blocking(move || {
            let dir = Dir::open_ambient_dir(&into, ambient_authority())?;
            afr_executor::tree::unpack(&dir, Path::new(WHOLE), &spool)
        })
        .await?
        .map(|_unpacked| ())
        .map_err(|source| error::boundary(DETAIL_EXPORT, source))
    }

    /// Imports a restored stage into the workspace `executor` serves, which
    /// must hold nothing yet: what lands is the checkpoint, nothing beside it.
    ///
    /// # Errors
    /// The workspace holds files, or the import failed.
    pub async fn restore(&self, executor: &dyn Executor) -> Result<Imported> {
        let listed = executor
            .list_dir(WHOLE)
            .await
            .map_err(|source| error::boundary(DETAIL_IMPORT, source))?;
        if !listed.entries.is_empty() || listed.truncated {
            return Err(error::config(DETAIL_NOT_EMPTY));
        }
        self.import(executor, WHOLE).await
    }

    /// A spool beside the stage that no name reaches.
    fn spool(&self) -> Result<File> {
        Ok(tempfile::tempfile_in(&self.spools)?)
    }
}

#[cfg(test)]
mod tests;
