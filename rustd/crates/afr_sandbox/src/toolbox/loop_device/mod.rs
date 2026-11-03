//! Loop devices, driven directly: an image is attached from the descriptor
//! that was hashed, never reopened by its path.
//!
//! `libc` declares neither the loop `ioctl`s nor `struct loop_config`, so both
//! are written out here from `<linux/loop.h>`; `LOOP_CONFIGURE` needs Linux
//! 5.8. The maintained loop-device crates build with `bindgen`, which would put
//! libclang on every Linux build of this workspace.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use backon::{BlockingRetryable as _, ConstantBuilder};

use crate::error::Result;
use crate::host::BLOCK_BYTES;
use crate::workspace_disk::Caching;

/// The device that hands out free loop devices.
const LOOP_CONTROL: &str = "/dev/loop-control";
/// Every loop device's node, before its number.
const LOOP_NODE_PREFIX: &str = "/dev/loop";
/// `LOOP_CTL_GET_FREE`: the number of a free loop device.
const LOOP_CTL_GET_FREE: libc::Ioctl = 0x4C82;
/// `LOOP_CONFIGURE`: attach a file and set its flags in one call.
const LOOP_CONFIGURE: libc::Ioctl = 0x4C0A;
/// `LOOP_GET_STATUS64`: what a device is attached to.
const LOOP_GET_STATUS64: libc::Ioctl = 0x4C05;
/// The block-device major number every loop device has.
pub(crate) const LOOP_MAJOR: u32 = 7;
/// Where the kernel publishes every block device by number, and the file
/// naming a device's node.
const SYS_DEV_BLOCK: &str = "/sys/dev/block";
const UEVENT: &str = "uevent";
/// The line of a device's `uevent` that names its node under `/dev`.
const DEVNAME: &str = "DEVNAME=";
/// `LO_FLAGS_READ_ONLY`: the device refuses writes.
pub(crate) const LO_FLAGS_READ_ONLY: u32 = 1;
/// `LO_FLAGS_AUTOCLEAR`: the device detaches itself once nothing holds it.
const LO_FLAGS_AUTOCLEAR: u32 = 4;
/// `LO_FLAGS_DIRECT_IO`: the device reads and writes its file past the host's
/// page cache, so its blocks are cached once, by whoever reads the device.
pub(crate) const LO_FLAGS_DIRECT_IO: u32 = 16;
/// `LO_NAME_SIZE` and `LO_KEY_SIZE`.
const LO_NAME_SIZE: usize = 64;
const LO_KEY_SIZE: usize = 32;
/// How many more free devices to try after the first: another process may
/// take the one the kernel offered before this one configures it.
const ATTACH_RETRIES: usize = 7;

/// `struct loop_info64`, its fields named without the header's `lo_` prefix.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LoopInfo64 {
    device: u64,
    inode: u64,
    rdevice: u64,
    offset: u64,
    sizelimit: u64,
    number: u32,
    encrypt_type: u32,
    encrypt_key_size: u32,
    flags: u32,
    file_name: [u8; LO_NAME_SIZE],
    crypt_name: [u8; LO_NAME_SIZE],
    encrypt_key: [u8; LO_KEY_SIZE],
    init: [u64; 2],
}

impl LoopInfo64 {
    /// Every field zero, as the kernel reads an unset one.
    const ZERO: Self = Self {
        device: 0,
        inode: 0,
        rdevice: 0,
        offset: 0,
        sizelimit: 0,
        number: 0,
        encrypt_type: 0,
        encrypt_key_size: 0,
        flags: 0,
        file_name: [0; LO_NAME_SIZE],
        crypt_name: [0; LO_NAME_SIZE],
        encrypt_key: [0; LO_KEY_SIZE],
        init: [0; 2],
    };
}

/// `struct loop_config`.
#[repr(C)]
#[derive(Debug)]
struct LoopConfig {
    fd: u32,
    block_size: u32,
    info: LoopInfo64,
    reserved: [u64; 8],
}

// The layouts the kernel reads and writes through the pointers below.
const _: () = assert!(size_of::<LoopInfo64>() == 232);
const _: () = assert!(size_of::<LoopConfig>() == 304);

/// What a loop device is attached to, as the kernel reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Backing {
    /// The device and inode of the file behind it, as the kernel encodes them.
    pub(crate) file: (u64, u64),
    /// Its `LO_FLAGS_*`.
    pub(crate) flags: u32,
    /// Where in the file it starts; zero when it shows the file from the top.
    pub(crate) offset: u64,
    /// How much of the file it shows; zero when it shows all of it.
    pub(crate) size_limit: u64,
}

/// Whether a loop device may write its image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    /// The toolbox: every lease reads it, none writes it.
    ReadOnly,
    /// A lease's own workspace disk.
    ReadWrite,
}

impl Access {
    const fn flags(self) -> u32 {
        match self {
            Self::ReadOnly => LO_FLAGS_READ_ONLY,
            Self::ReadWrite => 0,
        }
    }

    /// Whether the device itself is opened for writing. `LOOP_CONFIGURE`
    /// makes a device read-only when the handle configuring it cannot write,
    /// whatever the flags ask.
    const fn writes(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

/// A loop device attached to an image, held open until its mount takes it:
/// with autoclear set, closing the last handle before then would detach it.
#[derive(Debug)]
pub(crate) struct Attached {
    /// The device's node.
    pub(crate) node: PathBuf,
    /// Whether direct I/O took, as the kernel reads the device back.
    pub(crate) caching: Caching,
    /// The handle that keeps it attached.
    _device: File,
}

/// Attaches `image` to a free loop device from the descriptor itself, asking
/// for direct I/O and page-sized blocks in the one call. The device detaches
/// itself once its mount lets go of it. A kernel that refuses direct I/O
/// for this file (`EINVAL`) gets the attach again without it: a buffered
/// device, which [`Attached::caching`] says, not a failure.
pub(crate) fn attach(image: &File, access: Access) -> Result<Attached> {
    let control = OpenOptions::new()
        .read(true)
        .write(true)
        .open(LOOP_CONTROL)?;
    let fd = u32::try_from(image.as_raw_fd()).map_err(io::Error::other)?;
    let config = |direct| LoopConfig {
        fd,
        block_size: BLOCK_BYTES,
        info: LoopInfo64 {
            flags: access.flags() | LO_FLAGS_AUTOCLEAR | direct,
            ..LoopInfo64::ZERO
        },
        reserved: [0; 8],
    };
    let (direct, buffered) = (config(LO_FLAGS_DIRECT_IO), config(0));
    let attached = direct_or_buffered(
        || until_free(|| configure_free(&control, &direct, access)),
        || until_free(|| configure_free(&control, &buffered, access)),
    );
    Ok(attached?)
}

/// The attach `direct` makes, or `buffered`'s when the kernel refuses direct
/// I/O for this file (`EINVAL`): a buffered device, not a failure. Any other
/// refusal is the answer.
fn direct_or_buffered<T>(
    direct: impl FnOnce() -> io::Result<T>,
    buffered: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    match direct() {
        Err(refused) if refused.raw_os_error() == Some(libc::EINVAL) => buffered(),
        attached => attached,
    }
}

/// How a device whose flags read `flags` caches its file.
pub(crate) const fn caching(flags: u32) -> Caching {
    if flags & LO_FLAGS_DIRECT_IO == 0 {
        Caching::Buffered
    } else {
        Caching::Direct
    }
}

/// Why one try at attaching failed. Only a device another process configured
/// first is worth asking the kernel for another; an errno alone cannot say
/// which call failed, so the try says it.
#[derive(Debug)]
enum Attempt {
    /// `LOOP_CONFIGURE` answered `EBUSY`: the device was taken meanwhile.
    Taken(io::Error),
    /// Any other failure, of any call, whatever its errno.
    Failed(io::Error),
}

impl Attempt {
    /// `LOOP_CONFIGURE`'s refusal: taken when it is `EBUSY`, final otherwise.
    fn configure(refused: io::Error) -> Self {
        if refused.raw_os_error() == Some(libc::EBUSY) {
            Self::Taken(refused)
        } else {
            Self::Failed(refused)
        }
    }

    /// Whether another free device is worth trying.
    const fn taken(&self) -> bool {
        matches!(self, Self::Taken(_))
    }

    /// The failure the caller reads.
    fn into_error(self) -> io::Error {
        match self {
            Self::Taken(error) | Self::Failed(error) => error,
        }
    }
}

/// `attempt`, tried again at once while the device it was offered is taken,
/// [`ATTACH_RETRIES`] times at most; the last failure is the one returned.
fn until_free<T>(attempt: impl FnMut() -> Result<T, Attempt>) -> io::Result<T> {
    attempt
        .retry(
            ConstantBuilder::default()
                .with_delay(Duration::ZERO)
                .with_max_times(ATTACH_RETRIES),
        )
        .sleep(std::thread::sleep)
        .when(Attempt::taken)
        .call()
        .map_err(Attempt::into_error)
}

/// Asks the kernel for a free loop device, opens it as `access` needs, and
/// configures it with `config`.
fn configure_free(
    control: &File,
    config: &LoopConfig,
    access: Access,
) -> Result<Attached, Attempt> {
    // SAFETY: LOOP_CTL_GET_FREE takes no argument; it returns a device
    // number, or -1 with errno set.
    let number = unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
    if number < 0 {
        return Err(Attempt::Failed(io::Error::last_os_error()));
    }
    let node = PathBuf::from(format!("{LOOP_NODE_PREFIX}{number}"));
    let device = OpenOptions::new()
        .read(true)
        .write(access.writes())
        .open(&node)
        .map_err(Attempt::Failed)?;
    // SAFETY: `config` is a `struct loop_config` laid out as the kernel
    // declares it (asserted above), alive for the call and only read.
    let configured = unsafe { libc::ioctl(device.as_raw_fd(), LOOP_CONFIGURE, &raw const *config) };
    if configured != 0 {
        return Err(Attempt::configure(io::Error::last_os_error()));
    }
    let flags = status(&device).map_err(Attempt::Failed)?.flags;
    Ok(Attached {
        node,
        caching: caching(flags),
        _device: device,
    })
}

/// What the loop device at `node` is attached to.
pub(crate) fn backing(node: &Path) -> Result<Backing> {
    Ok(status(&File::open(node)?)?)
}

/// What the open loop `device` is attached to.
fn status(device: &File) -> io::Result<Backing> {
    let mut info = LoopInfo64::ZERO;
    // SAFETY: LOOP_GET_STATUS64 writes one `struct loop_info64` through the
    // pointer, which is valid, laid out as the kernel declares it (asserted
    // above) and exclusively borrowed for the call.
    let answered = unsafe { libc::ioctl(device.as_raw_fd(), LOOP_GET_STATUS64, &raw mut info) };
    if answered != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Backing {
        file: (info.device, info.inode),
        flags: info.flags,
        offset: info.offset,
        size_limit: info.sizelimit,
    })
}

/// The node of loop device `major:minor`, as the kernel names it.
pub(crate) fn node(major: u32, minor: u32) -> Result<PathBuf> {
    node_in(Path::new(SYS_DEV_BLOCK), major, minor)
}

/// [`node`], read from the block devices listed under `sys_dev_block`.
fn node_in(sys_dev_block: &Path, major: u32, minor: u32) -> Result<PathBuf> {
    let uevent = fs::read_to_string(sys_dev_block.join(format!("{major}:{minor}")).join(UEVENT))?;
    uevent
        .lines()
        .find_map(|line| line.strip_prefix(DEVNAME))
        .map(|name| Path::new("/dev").join(name))
        .ok_or_else(|| {
            io::Error::other(format!("loop device {major}:{minor} names no node")).into()
        })
}

#[cfg(test)]
mod tests;
