//! The images a sandbox is built from: the signed toolbox every run sees as
//! its root, and the workspace disk each run writes to.
//!
//! [`Toolboxes`] verifies a staged release, publishes its image and mounts
//! it read-only; [`format_image`] makes and formats one run's ext4 image.
//! [`HostTools`] names the host programs an engine runs. Both images are
//! block files, which a microVM engine attaches as drives.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod host;
#[cfg(target_os = "linux")]
mod mounts;
mod probe;
mod toolbox;
mod workspace_disk;

pub use self::error::{Error, Result, ToolboxRefusal};
pub use self::host::{HostTools, MKE2FS_PATH, tail};
pub use self::probe::{FILESYSTEMS_PATH, direct_io, toolbox_filesystem};
#[cfg(target_os = "linux")]
pub use self::toolbox::KernelMounter;
pub use self::toolbox::{
    MANIFEST_SUFFIX, Manifest, MountedToolboxes, Mounter, Release, SIGNATURE_SUFFIX,
    TOOLBOX_KEEP_RELEASES, TOOLBOX_PREFIX, TOOLBOX_RELEASE_PUBLIC_KEY, TOOLBOX_SUFFIX, Toolbox,
    ToolboxHome, Toolboxes,
};
pub use self::workspace_disk::{Caching, format_image};
