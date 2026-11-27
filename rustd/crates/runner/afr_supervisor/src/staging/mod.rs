//! Where a sandbox's files wait on the host on their way in or out.
//!
//! A repository is checked out here before it is imported, a workspace is
//! exported here before a checkpoint reads it, and a checkpoint is restored
//! here before it is imported.
//!
//! ```text
//!   <home>/staging/<sandbox>-<random>/      one step's tree: 0700, the runner's,
//!                                           removed when the step ends
//!   <home>/staging/<sandbox>-<random>.pin   one save's pinned bytes: 0600, the
//!                                           runner's, removed when it is released
//!   <home>/staging/                         emptied at boot of what a crash left
//! ```
//!
//! The workspace itself stays the engine's: files cross only through the
//! guest agent's `import` and `export`, and both ends check the stream with the
//! same [`af_guest_wire::tree`] functions. A tree crosses through an anonymous
//! spool beside the stage, never inside it, so no spool is packed or
//! unpacked over.

use std::fs::{self, File};
use std::io::{self, Seek as _, SeekFrom};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use af_core::id::Uuid7;
use af_guest_wire::Imported;
use afr_guest_client::GuestAgent;
use cap_std::ambient_authority;
use cap_std::fs::Dir;

use self::bounded::Bounded;
use crate::error::{self, Result};

mod bounded;
mod volume;

pub(crate) use self::volume::{ExportPin, ExportVolume, FREEZE_TENANT};

/// Only the runner reads or writes a stage: a tenant's files pass through it.
const STAGE_MODE: u32 = 0o700;
/// Only the runner reads or writes a pin's bytes: every file the run wrote.
const PIN_MODE: u32 = 0o600;
/// The event a stage the boot sweep could not remove is logged under.
const EVENT_SWEEP_LEFT: &str = "staging_sweep_left";
/// The workspace, and a stage, named relative to itself.
const WHOLE: &str = ".";
/// Why a stage could not be filled from the workspace.
const DETAIL_EXPORT: &str = "the workspace would not export into its stage";
/// Why an export that outgrew the workspace's own disk was cut off.
const DETAIL_OUTGROWN: &str = "the workspace's export outgrew the disk the run was granted";
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

    /// A fresh, empty file for one save's pinned bytes of `sandbox`, removed
    /// when dropped.
    ///
    /// # Errors
    /// The file could not be made.
    pub(crate) fn pin_file(&self, sandbox: &Uuid7) -> Result<tempfile::NamedTempFile> {
        Ok(tempfile::Builder::new()
            .prefix(&format!("{sandbox}-"))
            .suffix(".pin")
            .permissions(fs::Permissions::from_mode(PIN_MODE))
            .tempfile_in(&self.root)?)
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
    /// workspace `agent` serves; `.` is the whole stage and workspace.
    ///
    /// # Errors
    /// The stage would not pack, or the guest agent refused the stream.
    pub async fn import(&self, agent: &dyn GuestAgent, at: &str) -> Result<Imported> {
        let (source, spool) = (self.path().join(at), self.spool()?);
        let packed = tokio::task::spawn_blocking(move || {
            let dir = Dir::open_ambient_dir(&source, ambient_authority())?;
            af_guest_wire::tree::pack(&dir, &[Path::new(WHOLE)], &spool)?;
            let mut rewind = &spool;
            rewind.seek(SeekFrom::Start(0))?;
            Ok::<_, af_guest_wire::Error>(spool)
        })
        .await?
        .map_err(|source| error::boundary(DETAIL_IMPORT, source))?;
        let mut stream = tokio::fs::File::from_std(packed);
        agent
            .import(at, &mut stream)
            .await
            .map_err(|source| error::boundary(DETAIL_IMPORT, source))
    }

    /// Exports the whole workspace `agent` serves and unpacks it into the
    /// stage, every entry checked as an import checks it.
    ///
    /// The stream is spooled on the host, so it is held to `disk_bytes`, the
    /// workspace disk the run was granted. A pack carries each byte the tree
    /// holds on that disk once, a second name as a link and a hole not at
    /// all, so a longer stream is no pack of it: its guest is sending bytes
    /// it does not hold, or the tree is more names than content, each entry's
    /// 512-byte header against a directory entry a fraction of that. The
    /// unpack writes no more than the stream held, a hole landing as a hole;
    /// but whoever reads the stage reads a hole as bytes, so the content its
    /// files claim, holes included, is held to the same disk. Past either,
    /// the export ends as too large and its stage is the caller's to drop.
    ///
    /// # Errors
    /// The guest agent would not export; the stream, or the files it claims,
    /// outgrew `disk_bytes`, which is the checkpoint-limit class; or an entry
    /// would leave the stage. The spool goes with the call either way.
    pub async fn export(&self, agent: &dyn GuestAgent, disk_bytes: u64) -> Result<()> {
        let spool = spool_export(agent, disk_bytes, self.spool()?).await?;
        self.unpack(spool, disk_bytes).await
    }

    /// Unpacks the export `spool` holds into the stage, every entry checked
    /// as an import checks it, and holds what its files claim to `disk_bytes`.
    ///
    /// # Errors
    /// As [`Staged::export`], past the guest agent's own refusal.
    pub(crate) async fn unpack(&self, spool: File, disk_bytes: u64) -> Result<()> {
        let into = self.path().to_owned();
        let unpacked = tokio::task::spawn_blocking(move || {
            let dir = Dir::open_ambient_dir(&into, ambient_authority())?;
            af_guest_wire::tree::unpack(&dir, Path::new(WHOLE), &spool)
        })
        .await?
        .map_err(|source| error::boundary(DETAIL_EXPORT, source))?;
        if unpacked.bytes > disk_bytes {
            return Err(error::capture_limit(DETAIL_OUTGROWN));
        }
        Ok(())
    }

    /// Imports a restored stage into the workspace `agent` serves, which
    /// must hold nothing yet: what lands is the checkpoint, nothing beside it.
    ///
    /// # Errors
    /// The workspace holds files, or the import failed.
    pub async fn restore(&self, agent: &dyn GuestAgent) -> Result<Imported> {
        let listed = agent
            .list_dir(WHOLE)
            .await
            .map_err(|source| error::boundary(DETAIL_IMPORT, source))?;
        if !listed.entries.is_empty() || listed.truncated {
            return Err(error::config(DETAIL_NOT_EMPTY));
        }
        self.import(agent, WHOLE).await
    }

    /// A spool beside the stage that no name reaches.
    fn spool(&self) -> Result<File> {
        Ok(tempfile::tempfile_in(&self.spools)?)
    }
}

/// Exports the whole workspace `agent` serves into `spool`, held to
/// `disk_bytes` as [`Staged::export`] says, and hands the spool back.
///
/// # Errors
/// The guest agent would not export, or the stream outgrew `disk_bytes`.
pub(crate) async fn spool_export(
    agent: &dyn GuestAgent,
    disk_bytes: u64,
    spool: File,
) -> Result<File> {
    let mut stream = Bounded::new(tokio::fs::File::from_std(spool), disk_bytes);
    let exported = agent.export(&[WHOLE], &mut stream).await;
    if stream.outgrown() {
        return Err(error::capture_limit(DETAIL_OUTGROWN));
    }
    exported.map_err(|source| error::boundary(DETAIL_EXPORT, source))?;
    Ok(stream.into_spool().into_std().await)
}

#[cfg(test)]
mod tests;
