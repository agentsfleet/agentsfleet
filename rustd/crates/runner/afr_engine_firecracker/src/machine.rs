//! One boot's `--config-file`, as Firecracker reads it.
//!
//! The guest kernel and its command line, the drives, the machine's size, the
//! tap and the vsock device. Every path is inside the jail, which the monitor
//! sees as `/`.
//!
//! ```text
//!   vda  toolbox    the verified loop device, read-only
//!   vdb  workspace  the run's ext4 image, read-write, Writeback, rate-limited
//!   vdc  names      the plan's /etc/hosts and /etc/resolv.conf, read-only
//!   eth0            the tap, when the plan reaches a network
//!   vsock           guest 3, the host's end a Unix socket in the jail
//! ```

use std::ffi::OsString;
use std::net::Ipv4Addr;

use afr_engine::Granted;
use serde::Serialize;

use crate::capabilities::{memory_mib, vcpus};

/// Where each file the monitor opens sits in its jail.
pub const KERNEL: &str = "/vmlinux";
/// See [`KERNEL`].
pub const INITRD: &str = "/initrd";
/// See [`KERNEL`].
pub const TOOLBOX: &str = "/toolbox";
/// See [`KERNEL`].
pub const WORKSPACE: &str = "/workspace.img";
/// See [`KERNEL`].
pub const NAMES: &str = "/names";
/// See [`KERNEL`].
pub const VSOCK: &str = "/v.sock";
/// See [`KERNEL`].
pub const CONFIG: &str = "/config.json";
/// The guest's context identifier: the lowest a guest may take, the vsock
/// address family keeping 0 to 2 for itself. Each monitor's vsock is its own
/// device, so every guest takes the same one.
pub const GUEST_CID: u32 = 3;
/// The milliseconds a drive's byte budget refills over: one second, so the
/// budget is the configured bytes per second.
const REFILL_MILLIS: u64 = 1_000;
/// The guest's network device, as its kernel names the first one.
const GUEST_DEVICE: &str = "eth0";
/// The kernel's command line before the guest's own: a serial console,
/// a reboot that ends the monitor, and a panic that ends the guest at once.
const KERNEL_ARGS: &str = "console=ttyS0 quiet reboot=k panic=1 pci=off";

/// What one boot is built from.
#[derive(Debug, Clone, Copy)]
pub struct Boot<'a> {
    /// What the guest enforces.
    pub granted: &'a Granted,
    /// What the guest entry is told, after the kernel's own arguments.
    pub entry: &'a [OsString],
    /// The workspace drive's byte budget, each second.
    pub io_bytes_per_second: u64,
    /// The guest's address and gateway on its tap, when the plan reaches a
    /// network; no tap otherwise.
    pub network: Option<GuestNetwork>,
    /// Whether the plan's names ride a drive of their own.
    pub names: bool,
}

/// A guest's address on its tap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestNetwork {
    /// The guest's own address.
    pub address: Ipv4Addr,
    /// The tap's, which routes everything.
    pub gateway: Ipv4Addr,
    /// The network both sit in.
    pub prefix_len: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Document {
    boot_source: BootSource,
    drives: Vec<Drive>,
    machine_config: MachineConfig,
    network_interfaces: Vec<NetworkInterface>,
    vsock: Vsock,
}

#[derive(Serialize)]
struct BootSource {
    kernel_image_path: &'static str,
    initrd_path: &'static str,
    boot_args: String,
}

#[derive(Serialize)]
struct Drive {
    #[serde(rename = "drive_id")]
    id: &'static str,
    path_on_host: &'static str,
    is_root_device: bool,
    is_read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limiter: Option<RateLimiter>,
}

#[derive(Serialize)]
struct RateLimiter {
    bandwidth: TokenBucket,
}

#[derive(Serialize)]
struct TokenBucket {
    size: u64,
    refill_time: u64,
}

#[derive(Serialize)]
struct MachineConfig {
    vcpu_count: u32,
    mem_size_mib: u64,
}

#[derive(Serialize)]
struct NetworkInterface {
    iface_id: &'static str,
    host_dev_name: &'static str,
}

#[derive(Serialize)]
struct Vsock {
    guest_cid: u32,
    uds_path: &'static str,
}

/// The configuration file `boot` starts its monitor from.
///
/// # Errors
/// The encoder refused it, which no value here can make it do.
pub fn render(boot: &Boot<'_>, tap: &'static str) -> serde_json::Result<String> {
    let read_only = |id, path_on_host| Drive {
        id,
        path_on_host,
        is_root_device: false,
        is_read_only: true,
        cache_type: None,
        rate_limiter: None,
    };
    let mut drives = vec![
        read_only("toolbox", TOOLBOX),
        Drive {
            id: "workspace",
            path_on_host: WORKSPACE,
            is_root_device: false,
            is_read_only: false,
            // A guest's synced write reaches the image before the monitor
            // answers the flush, so a stop after a sync loses nothing.
            cache_type: Some("Writeback"),
            rate_limiter: Some(RateLimiter {
                bandwidth: TokenBucket {
                    size: boot.io_bytes_per_second,
                    refill_time: REFILL_MILLIS,
                },
            }),
        },
    ];
    if boot.names {
        drives.push(read_only("names", NAMES));
    }
    serde_json::to_string_pretty(&Document {
        boot_source: BootSource {
            kernel_image_path: KERNEL,
            initrd_path: INITRD,
            boot_args: boot_args(boot),
        },
        drives,
        machine_config: MachineConfig {
            vcpu_count: vcpus(boot.granted),
            mem_size_mib: memory_mib(boot.granted),
        },
        network_interfaces: boot
            .network
            .map(|_network| NetworkInterface {
                iface_id: GUEST_DEVICE,
                host_dev_name: tap,
            })
            .into_iter()
            .collect(),
        vsock: Vsock {
            guest_cid: GUEST_CID,
            uds_path: VSOCK,
        },
    })
}

/// The kernel's command line: its own arguments, the guest's address when it
/// has a tap, then `--` and what the guest entry is told.
fn boot_args(boot: &Boot<'_>) -> String {
    let address = boot.network.map(|network| {
        let mask = Ipv4Addr::from(
            u32::MAX
                .checked_shl(32 - u32::from(network.prefix_len))
                .unwrap_or(0),
        );
        format!(
            " ip={}::{}:{mask}::{GUEST_DEVICE}:off",
            network.address, network.gateway
        )
    });
    let entry: Vec<String> = boot
        .entry
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    format!(
        "{KERNEL_ARGS}{} -- {}",
        address.unwrap_or_default(),
        entry.join(" ")
    )
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
