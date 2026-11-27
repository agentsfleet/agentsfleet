//! One boot of one microVM, on a fresh build or a resume, and the one release
//! every boot ends with.
//!
//! In order: a jail user, the image's lock, the bound namespace and its tap
//! and host table, the chroot, the monitor's cgroup leaf, then the jailer.
//! Each piece is kept in [`Parts`] as it is made, so a boot that fails
//! releases exactly what it made, and a parked or destroyed sandbox releases
//! the same pieces the same way.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};

use af_core::error_code::{Coded as _, Logged};
use afr_cgroup::{CGROUP_PROCS, RunCgroup, SANDBOX_MEMORY_RESERVE_BYTES};
use afr_engine::{Allowlist, Limits, RESOLV_CONF, Result, until_in_force};
use afr_executor::{Client, Executor as _};
use afr_firewall::{
    Destinations, GUEST_ADDRESS, GUEST_PREFIX_LEN, GuestNamespace, HostScope, TAP, TAP_ADDRESS,
};
use tokio::process::{Child, Command};

use super::vm::Vm;
use super::{Kept, Reach, Shared};
use crate::admission::Reservation;
use crate::error::{exited, not_ready, unrendered};
use crate::guest::{EXECUTOR_PORT, GuestEntry};
use crate::jail::{self, Furnishing, JailId};
use crate::lock::ImageLock;
use crate::machine::{self, Boot, GuestNetwork};
use crate::vsock::VsockRoute;

/// What a process writes to `cgroup.procs` to move itself.
const SELF: &[u8] = b"0";
/// What the ready probe lists: the workspace, the executor's root.
const READY_PROBE: &str = ".";
/// The event a piece a release could not remove is logged under.
const EVENT_RELEASE_FAILED: &str = "sandbox_teardown_failed";

/// Everything one boot holds while its monitor runs; released once.
#[derive(Debug, Default)]
pub(crate) struct Parts {
    reservation: Option<Reservation>,
    jail: Option<JailId>,
    lock: Option<ImageLock>,
    namespace: Option<GuestNamespace>,
    pub(super) scope: Option<HostScope>,
    cgroup: Option<RunCgroup>,
    /// The jailer's process, then the monitor's; behind a lock so whether it
    /// still runs is read through `&self`.
    child: Mutex<Option<Child>>,
}

/// Boots the sandbox `kept` describes on its kept disk, held to its plan,
/// and returns once its executor answers and the plan reads back in force.
/// It holds `reservation`, its share of the host's admission bound, until it is
/// released.
///
/// # Errors
/// Any step failed; what the boot made is released before the error returns,
/// and the sandbox's directory and image stay for the caller.
pub(crate) async fn boot(shared: &Arc<Shared>, kept: Kept, reservation: Reservation) -> Result<Vm> {
    let mut parts = Parts {
        reservation: Some(reservation),
        ..Parts::default()
    };
    let client = match start(shared, &kept, &mut parts).await {
        Ok(client) => client,
        Err(error) => {
            parts.release(&kept).await;
            return Err(error);
        }
    };
    let generation = kept.plan.generation;
    let vm = Vm::new(Arc::clone(shared), kept, client, parts);
    let within = shared.config.tuning.ready_timeout;
    match until_in_force(&vm, generation, within).await {
        Ok(()) => Ok(vm),
        Err(error) => {
            let _logged = Box::new(vm).release().await;
            Err(error)
        }
    }
}

/// Makes every piece of one boot into `parts`, then dials its executor.
async fn start(shared: &Shared, kept: &Kept, parts: &mut Parts) -> Result<Client> {
    let config = &shared.config;
    let jail = parts.jail.insert(shared.jails.claim()?);
    let owner = jail.owner();
    let image = kept.layout.image();
    parts.lock = Some(ImageLock::take(&image)?);
    jail::own(&image, owner, jail::FILE_MODE)?;
    let namespace = parts
        .namespace
        .insert(GuestNamespace::create(&kept.layout.namespace())?);
    let network = join(namespace, &kept.plan.reach, owner, &mut parts.scope)?;
    let entry = GuestEntry::new(
        kept.granted.limits().shared_memory_bytes(),
        config.tuning.sizes,
    )
    .arguments();
    let names = names(&kept.plan.reach);
    let boot = Boot {
        granted: &kept.granted,
        entry: &entry,
        io_bytes_per_second: config.tuning.io_bytes_per_second,
        network,
        names: names.is_some(),
    };
    let rendered = machine::render(&boot, TAP).map_err(unrendered)?;
    let with = Furnishing {
        binaries: &config.tuning.binaries,
        toolbox: config.toolbox.device()?,
        config: &rendered,
        names: names.as_deref(),
    };
    jail::furnish(&kept.layout, owner, with)?;
    let cgroup = parts.cgroup.insert(RunCgroup::create(
        &config.cgroup_root,
        kept.id.as_str(),
        &monitor_limits(kept.granted.limits()),
    )?);
    let argv = jail::jailer_arguments(
        &kept.layout,
        kept.id.as_str(),
        owner,
        &config.tuning.binaries,
    );
    let held = parts
        .child
        .get_mut()
        .unwrap_or_else(PoisonError::into_inner);
    let child = held.insert(spawn(
        &config.tuning.binaries.jailer,
        argv,
        &cgroup.procs(),
    )?);
    ready(child, &kept.layout, owner, config).await
}

/// Gives the guest a tap and joins its namespace to the host when `reach`
/// reaches a network, keeping the host scope in `scope`; says where the
/// guest sits on it.
fn join(
    namespace: &GuestNamespace,
    reach: &Reach,
    owner: (u32, u32),
    scope: &mut Option<HostScope>,
) -> Result<Option<GuestNetwork>> {
    let destinations = match reach {
        Reach::Isolated => return Ok(None),
        Reach::Public => Destinations::Public,
        Reach::Allowed(allowlist) => Destinations::Listed(allowlist),
    };
    namespace.attach_tap(owner)?;
    *scope = Some(HostScope::guarded(namespace.fd(), destinations)?);
    Ok(Some(GuestNetwork {
        address: GUEST_ADDRESS,
        gateway: TAP_ADDRESS,
        prefix_len: GUEST_PREFIX_LEN,
    }))
}

/// The names drive's text for `reach`: its hosts file, a NUL, and the
/// resolver-less resolver file. A public plan keeps the toolbox's own.
fn names(reach: &Reach) -> Option<String> {
    let hosts = match reach {
        Reach::Public => return None,
        Reach::Isolated => Allowlist::new(Vec::new()).ok()?.hosts_file(),
        Reach::Allowed(allowlist) => allowlist.hosts_file(),
    };
    Some(format!("{hosts}\0{RESOLV_CONF}"))
}

/// The monitor's cgroup: the guest's memory and the room a monitor's own
/// resident set takes beside it, the guest's processors, and the run's
/// process cap over the monitor's threads. `afr_cgroup` throttles the
/// monitor at this memory and caps it `WRITEBACK_HEADROOM_BYTES` above, and
/// admission reserves this memory, not the cap.
pub(super) fn monitor_limits(granted: &Limits) -> Limits {
    Limits {
        memory_bytes: granted
            .memory_bytes
            .saturating_add(SANDBOX_MEMORY_RESERVE_BYTES),
        ..*granted
    }
}

/// Starts the jailer with `argv` inside the cgroup whose `cgroup.procs` is
/// `procs`, so the monitor it becomes is born there.
fn spawn(jailer: &Path, argv: Vec<std::ffi::OsString>, procs: &Path) -> Result<Child> {
    let join = fs::OpenOptions::new()
        .write(true)
        .open(procs)
        .map_err(afr_cgroup::error::cgroup(CGROUP_PROCS))?;
    let mut command = Command::new(jailer);
    // The monitor needs nothing of the root runner's environment, its
    // credential least of all.
    command
        .args(argv)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let hook = move || {
        rustix::io::write(&join, SELF)
            .map(drop)
            .map_err(io::Error::from)
    };
    // SAFETY: the hook runs in the child between fork and exec, where only
    // async-signal-safe calls are sound. It makes one `write` system call on
    // a file opened before the fork and owned by the hook, and allocates
    // nothing, so the jailer, and the monitor it execs, is born in the leaf.
    unsafe { command.pre_exec(hook) };
    Ok(command.spawn()?)
}

/// The guest's executor, once it lists its workspace, dialled through the
/// socket its monitor, running as `owner`, made in `layout`'s jail. A monitor
/// that exits first is refused with how it ended, and a guest silent past the
/// ready timeout as not ready.
async fn ready(
    child: &mut Child,
    layout: &jail::Layout,
    (uid, _gid): (u32, u32),
    config: &super::FirecrackerConfig,
) -> Result<Client> {
    let waited = config.tuning.ready_timeout;
    let (jail, socket) = layout.socket();
    let route = VsockRoute::new(jail.to_owned(), socket, uid, EXECUTOR_PORT, waited);
    let answered = async {
        let client = Client::dial(route, config.tuning.sizes).await?;
        client.list_dir(READY_PROBE).await?;
        Ok::<_, afr_executor::Error>(client)
    };
    let listed = tokio::select! {
        status = child.wait() => return Err(status.map_or_else(Into::into, exited)),
        listed = tokio::time::timeout(waited, answered) => listed,
    };
    match listed {
        Ok(Ok(client)) => Ok(client),
        Ok(Err(lost)) => Err(not_ready(waited)(lost)),
        Err(_elapsed) => Err(not_ready(waited)(
            io::Error::from(io::ErrorKind::TimedOut).into(),
        )),
    }
}

impl Parts {
    /// Whether the monitor still runs. The first look that finds it ended
    /// reaps it and tells `ended` how it ended; later looks say only that it
    /// does not run.
    pub(super) fn running(&self, ended: impl FnOnce(&str)) -> bool {
        let mut held = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(child) = held.as_mut() else {
            return false;
        };
        let how = match child.try_wait() {
            Ok(None) => return true,
            Ok(Some(status)) => status.to_string(),
            Err(unread) => unread.to_string(),
        };
        *held = None;
        drop(held);
        ended(&how);
        false
    }

    /// The monitor's process identifier, while it is held.
    pub(super) fn monitor_pid(&self) -> Option<u32> {
        self.child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(Child::id)
    }

    /// The monitor's process, when it was started and not yet reaped.
    /// Nothing panics while the lock is held, so a poisoned one still holds
    /// it whole.
    fn child(&mut self) -> &mut Option<Child> {
        self.child.get_mut().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stops the monitor and releases every piece of the boot in the order
    /// they were made, last first; the image and the sandbox's directory stay.
    /// What could not be released is logged and left for the boot sweep.
    pub(super) async fn release(&mut self, kept: &Kept) {
        if let Some(mut child) = self.child().take() {
            let _gone = child.start_kill();
            let _reaped = child.wait().await;
        }
        let cgroup = self.cgroup.take();
        let removed = tokio::task::spawn_blocking(move || cgroup.map(RunCgroup::remove)).await;
        if let Ok(Some(Err(error))) = removed {
            logged(kept, &error.into());
        }
        if let Some(scope) = self.scope.take()
            && let Err(error) = scope.remove()
        {
            logged(kept, &error.into());
        }
        if let Some(namespace) = self.namespace.take()
            && let Err(error) = namespace.remove()
        {
            logged(kept, &error.into());
        }
        if let Err(error) = kept.layout.clear_jail() {
            logged(kept, &error.into());
        }
        self.lock = None;
        self.jail = None;
        self.reservation = None;
    }
}

/// Logs a piece of `kept`'s boot that a release could not remove.
fn logged(kept: &Kept, error: &afr_engine::Error) {
    let sandbox_id = kept.id.as_str();
    let Logged { error_code, reason } = error.logged();
    let event = EVENT_RELEASE_FAILED;
    tracing::warn!(sandbox_id, error_code, reason, event);
}
