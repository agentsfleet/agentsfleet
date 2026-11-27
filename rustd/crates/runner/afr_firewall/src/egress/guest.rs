//! A microVM's network: a namespace of its own, bound at a path its jailer
//! joins, holding the tap its virtual machine monitor opens, and joined to the
//! host by a slot's veth pair and table as a shared-kernel sandbox is.
//!
//! ```text
//!   guest            slot namespace                       host namespace
//!   eth0 .2/30 ─tap─ tap0 172.16.0.1/30  afpN 10.69.N.2 ─veth─ afvN: table afegressN
//!                    table afguest: masquerade, input drop     reserved ranges dropped
//!                    forward: tap0 saddr != .2 drop            afvN saddr != 10.69.N.2 drop
//! ```
//!
//! The namespace's own table translates, and forwards from the tap only what
//! the guest's one address sends; every verdict on where the guest may go is
//! the host table's, which no guest and no monitor can reach.
//! The tap has no IPv6, and nothing it sends reaches the namespace itself.

use std::fs::{self, File};
use std::io;
use std::net::Ipv4Addr;
use std::os::fd::{AsFd as _, AsRawFd as _, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use rustix::mount::UnmountFlags;
use rustix::thread::UnshareFlags;

use super::kernel::{Host, Kernel as _, THREAD_NAMESPACE, on_own_thread};
use super::{link, rules};
use crate::error::{Result, Step, netlink};

/// The tap every guest's monitor opens: one per namespace, so one name.
pub const TAP: &str = "tap0";
/// The tap's address in its namespace: the guest's gateway.
pub const TAP_ADDRESS: Ipv4Addr = Ipv4Addr::new(172, 16, 0, 1);
/// The guest's own address.
pub const GUEST_ADDRESS: Ipv4Addr = Ipv4Addr::new(172, 16, 0, 2);
/// The tap's network: the gateway, the guest, and nothing else.
pub const GUEST_PREFIX_LEN: u8 = 30;
/// The table that translates the guest's traffic in its namespace.
const GUEST_TABLE: &str = "afguest";
/// The device every tap is made through.
const TUN_DEVICE: &str = "/dev/net/tun";
/// What a tap is asked for: a layer-two device, no packet header, and the
/// virtio header the monitor reads and writes.
const TAP_FLAGS: libc::c_int = libc::IFF_TAP | libc::IFF_NO_PI | libc::IFF_VNET_HDR;
/// The namespace's forwarding switch, read where the calling thread is.
const IP_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";
/// The tap's IPv6 switch; absent on a kernel without IPv6.
const TAP_IPV6_OFF: &str = "/proc/sys/net/ipv6/conf/tap0/disable_ipv6";
/// What both switches are written.
const ON: &str = "1";
/// The bound namespace file: only its owner reads it.
const NAMESPACE_MODE: u32 = 0o600;

/// Brings loopback up in the calling thread's network namespace.
///
/// A guest's kernel raises it only when it is booted with an address, so in
/// a guest with no network device a process dialling `127.0.0.1` reads the
/// network unreachable until its `init` does this.
///
/// # Errors
/// The kernel refused the route socket, the lookup or the change.
pub fn raise_loopback() -> Result<()> {
    let mut route = Host.route().map_err(netlink(Step::Loopback))?;
    link::raise_loopback(&mut route).map_err(netlink(Step::Loopback))
}

/// A network namespace bound at a path, so a jailer joins it by name; it
/// lives until [`GuestNamespace::remove`], or the boot sweep's
/// [`remove_namespace`], unbinds it and the last process in it ends.
#[derive(Debug)]
pub struct GuestNamespace {
    path: PathBuf,
    file: File,
}

impl GuestNamespace {
    /// A fresh namespace, with nothing in it but loopback, bound at `path`.
    ///
    /// # Errors
    /// `path` exists already, or the kernel refused the namespace or the bind;
    /// a file made before the refusal is removed.
    pub fn create(path: &Path) -> Result<Self> {
        let bound = on_own_thread(|| {
            // SAFETY: only the network namespace is unshared, and only on this
            // thread, which exists for this call alone; nothing the rest of the
            // process relies on changes. The bind below outlives the thread.
            unsafe { rustix::thread::unshare_unsafe(UnshareFlags::NEWNET) }
                .map_err(io::Error::from)?;
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(NAMESPACE_MODE)
                .open(path)?;
            rustix::mount::mount_bind(THREAD_NAMESPACE, path).map_err(io::Error::from)
        });
        if let Err(refused) = bound {
            let _absent = fs::remove_file(path);
            return Err(refused.into());
        }
        Ok(Self {
            path: path.to_owned(),
            file: File::open(path)?,
        })
    }

    /// Where it is bound: what the jailer's `--netns` names.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The namespace, held open.
    #[must_use]
    pub fn fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }

    /// Makes the tap a guest's monitor opens, owned by `owner` so the monitor
    /// running as that jail's user may open it, and routes what it sends out
    /// through the namespace's own address.
    ///
    /// # Errors
    /// The kernel refused a step; the tap and the table go with the namespace.
    pub fn attach_tap(&self, owner: (u32, u32)) -> Result<()> {
        Host.inside(self.fd(), || {
            make_tap(owner)?;
            let mut route = Host.route().map_err(netlink(Step::Tap))?;
            let index = link::index_named(&mut route, TAP).map_err(netlink(Step::Tap))?;
            route
                .acknowledged(vec![
                    link::address_within(index, TAP_ADDRESS, GUEST_PREFIX_LEN),
                    link::up(index),
                ])
                .map_err(netlink(Step::Tap))?;
            fs::write(IP_FORWARD, ON)?;
            if Path::new(TAP_IPV6_OFF).exists() {
                fs::write(TAP_IPV6_OFF, ON)?;
            }
            let network = Ipv4Addr::from(
                u32::from(TAP_ADDRESS) & (u32::MAX << (32 - u32::from(GUEST_PREFIX_LEN))),
            );
            Host.over_netfilter(Step::InstallRules, |netfilter| {
                netfilter.acknowledged(rules::install_guest_side(
                    GUEST_TABLE,
                    TAP,
                    GUEST_ADDRESS,
                    (network, GUEST_PREFIX_LEN),
                ))
            })
        })
    }

    /// Unbinds the namespace and removes its file: the tap, its table and
    /// the veth's peer go once the monitor in it has ended.
    ///
    /// # Errors
    /// The kernel refused the unbind, or the file would not go.
    pub fn remove(self) -> Result<()> {
        let Self { path, file } = self;
        drop(file);
        remove_namespace(&path)
    }
}

/// Unbinds whatever namespace is bound at `path` and removes the file; a file
/// already gone, or one nothing is bound to, is not a failure.
///
/// # Errors
/// The kernel refused the unbind for another reason, or the file would not go.
pub fn remove_namespace(path: &Path) -> Result<()> {
    match rustix::mount::unmount(path, UnmountFlags::DETACH) {
        Ok(()) | Err(rustix::io::Errno::INVAL | rustix::io::Errno::NOENT) => {}
        Err(refused) => return Err(io::Error::from(refused).into()),
    }
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
        _removed => Ok(()),
    }
}

/// Makes [`TAP`] in the calling thread's namespace, persistent and owned by
/// `owner`, so it outlives this call and the monitor's user may attach it.
///
/// The calls are made here: the kernel reads `TUNSETOWNER`, `TUNSETGROUP` and
/// `TUNSETPERSIST`'s argument as the value itself (`drivers/net/tun.c`), and
/// `tun-rs` 2.8 passes a pointer to it, which would own the tap by an address.
fn make_tap((uid, gid): (u32, u32)) -> io::Result<()> {
    let tun = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(TUN_DEVICE)?;
    let mut name = [0; libc::IFNAMSIZ];
    for (slot, byte) in name.iter_mut().zip(TAP.bytes()) {
        *slot = libc::c_char::from_le_bytes([byte]);
    }
    let mut request = libc::ifreq {
        ifr_name: name,
        ifr_ifru: libc::__c_anonymous_ifr_ifru {
            ifru_flags: libc::c_short::try_from(TAP_FLAGS).map_err(io::Error::other)?,
        },
    };
    let fd = tun.as_raw_fd();
    // SAFETY: `fd` is the open tun device, and `request` is a whole `ifreq`
    // the call reads and writes for its duration alone.
    ioctl_result(unsafe { libc::ioctl(fd, libc::TUNSETIFF, &raw mut request) })?;
    for (call, value) in [
        (libc::TUNSETOWNER, libc::c_ulong::from(uid)),
        (libc::TUNSETGROUP, libc::c_ulong::from(gid)),
        (libc::TUNSETPERSIST, 1),
    ] {
        // SAFETY: each call takes its argument by value on the device this
        // descriptor attached above, and touches no memory of this process.
        ioctl_result(unsafe { libc::ioctl(fd, call, value) })?;
    }
    Ok(())
}

/// An `ioctl`'s answer, as the error it set when it failed.
fn ioctl_result(answered: libc::c_int) -> io::Result<()> {
    if answered == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
