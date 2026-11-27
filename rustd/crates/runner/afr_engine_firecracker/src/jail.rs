//! One microVM's jail: its own user and group, the directory that holds it,
//! the chroot the stock jailer starts its monitor in, and the jailer's
//! command line.
//!
//! ```text
//!   <state>/<id>/                  0700 root   kept across a Stop park
//!     workspace.img                0600 jail   the guest's data drive, locked while it runs
//!     netns                        0600 root   the bound namespace the jailer joins
//!     jail/firecracker/<id>/root/  0700 jail   rebuilt every boot
//!       vmlinux initrd config.json names       0600 jail
//!       toolbox                    0600 jail   block node of the verified loop device
//!       workspace.img              the image, hard-linked
//!       v.sock                     the monitor's vsock socket, made by it
//! ```
//!
//! Every file in a jail is its user's alone, so a monitor cannot open another
//! jail's image, socket or chroot.

use std::ffi::OsString;
use std::fs::{self, DirBuilder, Permissions};
use std::io::{self, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use af_core::limits::MAX_WORKERS;
use afr_engine::Result;
use rustix::fs::{FileType, Gid, Mode, Uid};

use crate::config::Binaries;
use crate::error::jails_held;
use crate::machine::{CONFIG, INITRD, KERNEL, NAMES, TOOLBOX, VSOCK, WORKSPACE};

/// The workspace image's name in its sandbox's directory.
pub(crate) const IMAGE_FILE: &str = "workspace.img";
/// The bound namespace's name in its sandbox's directory.
pub(crate) const NAMESPACE_FILE: &str = "netns";
/// Where the jailer makes its chroots, in a sandbox's directory.
const JAIL_DIR: &str = "jail";
/// The chroot's own directory name, below the jailer's `<exec>/<id>/`.
const ROOT_DIR: &str = "root";
/// A jail's directories and files: its user's alone.
pub(crate) const DIR_MODE: u32 = 0o700;
pub(crate) const FILE_MODE: u32 = 0o600;
/// The sector a drive is read in: virtio-blk counts a drive in 512-byte
/// sectors, so a names drive is padded to a whole one or its tail is unread.
const SECTOR_BYTES: usize = 512;

/// The jail users and groups this host hands out: one per running microVM,
/// as many as a runner runs workers.
#[derive(Debug)]
pub(crate) struct JailIds {
    first: u32,
    held: Mutex<Vec<bool>>,
}

impl JailIds {
    /// The range starting at `first`.
    pub(crate) fn new(first: u32) -> Arc<Self> {
        Arc::new(Self {
            first,
            held: Mutex::new(vec![
                false;
                usize::try_from(MAX_WORKERS).unwrap_or(usize::MAX)
            ]),
        })
    }

    /// The lowest id no running microVM holds, held until dropped.
    pub(crate) fn claim(self: &Arc<Self>) -> Result<JailId> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let free = held.iter().position(|taken| !taken);
        let (Some(index), Ok(offset)) = (free, u32::try_from(free.unwrap_or_default())) else {
            return Err(jails_held(MAX_WORKERS));
        };
        if let Some(taken) = held.get_mut(index) {
            *taken = true;
        }
        Ok(JailId {
            ids: Arc::clone(self),
            index,
            id: self.first.saturating_add(offset),
        })
    }
}

/// One jail user and group, released when dropped.
#[derive(Debug)]
pub(crate) struct JailId {
    ids: Arc<JailIds>,
    index: usize,
    id: u32,
}

impl JailId {
    /// The user and group a monitor in this jail runs as and owns its files as.
    pub(crate) const fn owner(&self) -> (u32, u32) {
        (self.id, self.id)
    }
}

impl Drop for JailId {
    fn drop(&mut self) {
        let mut held = self.ids.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(taken) = held.get_mut(self.index) {
            *taken = false;
        }
    }
}

/// Where one sandbox's pieces live.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    dir: PathBuf,
    root: PathBuf,
}

impl Layout {
    /// The layout of sandbox `id` under `state_dir`, its monitor `firecracker`.
    pub(crate) fn new(state_dir: &Path, id: &str, firecracker: &Path) -> Self {
        let dir = state_dir.join(id);
        let exec = firecracker.file_name().unwrap_or_default();
        let root = dir.join(JAIL_DIR).join(exec).join(id).join(ROOT_DIR);
        Self { dir, root }
    }

    /// The sandbox's directory.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Its workspace image.
    pub(crate) fn image(&self) -> PathBuf {
        self.dir.join(IMAGE_FILE)
    }

    /// Where its namespace is bound.
    pub(crate) fn namespace(&self) -> PathBuf {
        self.dir.join(NAMESPACE_FILE)
    }

    /// The jail's chroot, and the monitor's vsock socket's name beneath it:
    /// the host reaches the socket by that name inside that directory, never
    /// by a path a monitor could redirect.
    pub(crate) fn socket(&self) -> (&Path, &'static str) {
        (&self.root, VSOCK.trim_start_matches('/'))
    }

    /// Where `path`, as the monitor names it, sits on the host.
    fn in_jail(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    /// Removes the chroot a boot used; the image and the directory stay.
    pub(crate) fn clear_jail(&self) -> io::Result<()> {
        match fs::remove_dir_all(self.dir.join(JAIL_DIR)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _cleared => Ok(()),
        }
    }
}

/// What one boot's chroot holds besides the image.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Furnishing<'a> {
    pub(crate) binaries: &'a Binaries,
    /// The toolbox's loop device, `major:minor`.
    pub(crate) toolbox: (u32, u32),
    pub(crate) config: &'a str,
    /// The names drive's text, when the plan has one.
    pub(crate) names: Option<&'a str>,
}

/// Builds a fresh chroot for one boot, every file `owner`'s alone.
///
/// # Errors
/// A file cannot be made, copied, linked or given its owner.
pub(crate) fn furnish(layout: &Layout, owner: (u32, u32), with: Furnishing<'_>) -> Result<()> {
    layout.clear_jail()?;
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(&layout.root)?;
    own(&layout.root, owner, DIR_MODE)?;
    for (from, to) in [
        (&with.binaries.kernel, KERNEL),
        (&with.binaries.initrd, INITRD),
    ] {
        let copy = layout.in_jail(to);
        fs::copy(from, &copy)?;
        own(&copy, owner, FILE_MODE)?;
    }
    let toolbox = layout.in_jail(TOOLBOX);
    let (major, minor) = with.toolbox;
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &toolbox,
        FileType::BlockDevice,
        Mode::from_raw_mode(FILE_MODE),
        rustix::fs::makedev(major, minor),
    )
    .map_err(io::Error::from)?;
    own(&toolbox, owner, FILE_MODE)?;
    fs::hard_link(layout.image(), layout.in_jail(WORKSPACE))?;
    write_owned(&layout.in_jail(CONFIG), with.config.as_bytes(), owner)?;
    if let Some(names) = with.names {
        let mut sectors = names.as_bytes().to_vec();
        sectors.resize(
            sectors.len().div_ceil(SECTOR_BYTES).max(1) * SECTOR_BYTES,
            0,
        );
        write_owned(&layout.in_jail(NAMES), &sectors, owner)?;
    }
    Ok(())
}

/// Gives `path` to `owner` with `mode`, whatever the umask made it.
pub(crate) fn own(path: &Path, (uid, gid): (u32, u32), mode: u32) -> io::Result<()> {
    rustix::fs::chown(path, Some(Uid::from_raw(uid)), Some(Gid::from_raw(gid)))?;
    fs::set_permissions(path, Permissions::from_mode(mode))
}

/// Writes `bytes` to a new file at `path`, `owner`'s alone.
fn write_owned(path: &Path, bytes: &[u8], owner: (u32, u32)) -> io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(path)?
        .write_all(bytes)?;
    own(path, owner, FILE_MODE)
}

/// The jailer's command line for sandbox `id`: its monitor, as `owner`, in
/// the chroot `layout` names and the namespace bound there, started from its
/// configuration file with no API socket.
pub(crate) fn jailer_arguments(
    layout: &Layout,
    id: &str,
    owner: (u32, u32),
    binaries: &Binaries,
) -> Vec<OsString> {
    let (uid, gid) = owner;
    vec![
        "--id".into(),
        id.into(),
        "--exec-file".into(),
        binaries.firecracker.clone().into(),
        "--uid".into(),
        uid.to_string().into(),
        "--gid".into(),
        gid.to_string().into(),
        "--chroot-base-dir".into(),
        layout.dir.join(JAIL_DIR).into(),
        "--netns".into(),
        layout.namespace().into(),
        "--".into(),
        "--config-file".into(),
        CONFIG.into(),
        "--no-api".into(),
    ]
}
