//! Loop devices, driven directly: an image is attached from the descriptor
//! that was hashed, never reopened by its path.
//!
//! `libc` declares neither the loop `ioctl`s nor `struct loop_config`, so both
//! are written out here from `<linux/loop.h>`; `LOOP_CONFIGURE` needs Linux
//! 5.8. The maintained loop-device crates build with `bindgen`, which would put
//! libclang on every Linux build of this workspace.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd as _;
use std::path::{Path, PathBuf};

use crate::error::Result;

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
/// `LO_FLAGS_READ_ONLY`: the device refuses writes.
pub(crate) const LO_FLAGS_READ_ONLY: u32 = 1;
/// `LO_FLAGS_AUTOCLEAR`: the device detaches itself once nothing holds it.
const LO_FLAGS_AUTOCLEAR: u32 = 4;
/// `LO_NAME_SIZE` and `LO_KEY_SIZE`.
const LO_NAME_SIZE: usize = 64;
const LO_KEY_SIZE: usize = 32;
/// How many free devices to try: another process may take the one the kernel
/// offered before this one configures it.
const ATTACH_ATTEMPTS: usize = 8;

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

/// A loop device attached to an image, held open until its mount takes it:
/// with autoclear set, closing the last handle before then would detach it.
#[derive(Debug)]
pub(crate) struct Attached {
    /// The device's node.
    pub(crate) node: PathBuf,
    /// The handle that keeps it attached.
    _device: File,
}

/// Attaches `image` read-only to a free loop device, from the descriptor
/// itself; the device detaches itself once its mount lets go of it.
pub(crate) fn attach(image: &File) -> Result<Attached> {
    let control = OpenOptions::new()
        .read(true)
        .write(true)
        .open(LOOP_CONTROL)?;
    let fd = u32::try_from(image.as_raw_fd()).map_err(io::Error::other)?;
    let config = LoopConfig {
        fd,
        block_size: 0,
        info: LoopInfo64 {
            flags: LO_FLAGS_READ_ONLY | LO_FLAGS_AUTOCLEAR,
            ..LoopInfo64::ZERO
        },
        reserved: [0; 8],
    };
    let mut busy = io::Error::from_raw_os_error(libc::EBUSY);
    for _attempt in 0..ATTACH_ATTEMPTS {
        // SAFETY: LOOP_CTL_GET_FREE takes no argument; it returns a device
        // number, or -1 with errno set.
        let number = unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
        if number < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let node = PathBuf::from(format!("{LOOP_NODE_PREFIX}{number}"));
        let device = File::open(&node)?;
        // SAFETY: `config` is a `struct loop_config` laid out as the kernel
        // declares it (asserted above), alive for the call and only read.
        let configured =
            unsafe { libc::ioctl(device.as_raw_fd(), LOOP_CONFIGURE, &raw const config) };
        if configured == 0 {
            return Ok(Attached {
                node,
                _device: device,
            });
        }
        busy = io::Error::last_os_error();
        if busy.raw_os_error() != Some(libc::EBUSY) {
            return Err(busy.into());
        }
    }
    Err(busy.into())
}

/// What the loop device at `node` is attached to.
pub(crate) fn backing(node: &Path) -> Result<Backing> {
    let device = File::open(node)?;
    let mut info = LoopInfo64::ZERO;
    // SAFETY: LOOP_GET_STATUS64 writes one `struct loop_info64` through the
    // pointer, which is valid, laid out as the kernel declares it (asserted
    // above) and exclusively borrowed for the call.
    let status = unsafe { libc::ioctl(device.as_raw_fd(), LOOP_GET_STATUS64, &raw mut info) };
    if status != 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(Backing {
        file: (info.device, info.inode),
        flags: info.flags,
        offset: info.offset,
        size_limit: info.sizelimit,
    })
}
