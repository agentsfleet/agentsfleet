//! What a sandboxed process may not do: hold a capability, make a refused
//! system call, write outside its workspace, or leave anything on the host
//! that host root owns.

use std::fs;
use std::os::unix::fs::MetadataExt as _;

use afr_sandbox::{
    Engine, Limits, MECHANISM_LANDLOCK as LANDLOCK, REFUSED_SYSCALLS, SandboxRequest,
    X32_SYSCALL_BIT,
};
use libtest_mimic::Failed;

use crate::lane::{Lane, SANDBOX_IDS};
use crate::run::{expect, in_sandbox, run, runtime, shell};

/// The lease the planting trial runs as.
const PLANT: &str = "plant";
/// Tries to leave a set-user-ID file in the socket directory, a host path,
/// then says who it runs as and writes one file where it may.
const PLANT_SCRIPT: &str = "(echo x > /run/agentsfleet/planted && chmod u+s /run/agentsfleet/planted) \
     2>/dev/null && echo planted || echo sealed; id -u; echo y > /workspace/owned";
/// Where `/workspace/owned` lands under the lease's directory: the disk mounts
/// at `workspace/`, and `/workspace` is its `workspace/` directory.
const OWNED_ON_HOST: &str = "workspace/workspace/owned";

/// Every system call the seccomp program refuses, by the filter's own list,
/// then `keyctl` with the x32 bit set, which a filter keyed on numbers alone
/// lets through wherever the kernel speaks x32.
fn refused() -> impl Iterator<Item = libc::c_long> {
    REFUSED_SYSCALLS
        .into_iter()
        .chain([libc::SYS_keyctl | libc::c_long::from(X32_SYSCALL_BIT)])
}
/// Calls each numbered system call and prints the errno each one set.
const PROBE_SYSCALLS: &str = "import ctypes, sys\nlibc = ctypes.CDLL(None, use_errno=True)\n\
     seen = []\nfor number in map(int, sys.argv[1:]):\n    ctypes.set_errno(0)\n    \
     libc.syscall(number, 0, 0, 0, 0, 0)\n    seen.append(ctypes.get_errno())\n\
     print(\" \".join(map(str, seen)))\n";

pub(crate) fn no_capabilities(lane: &Lane) -> Result<(), Failed> {
    let status = in_sandbox(lane, "caps", Limits::default(), "cat /proc/self/status")?;
    Ok(afr_sandbox::capabilities_dropped(&status.output)?)
}

pub(crate) fn seccomp_refuses(lane: &Lane) -> Result<(), Failed> {
    let numbers: Vec<String> = refused().map(|number| number.to_string()).collect();
    let script = format!("python3 -c \"$PROBE\" {}", numbers.join(" "));
    let script = format!("PROBE='{PROBE_SYSCALLS}'; {script}");
    let seen = in_sandbox(lane, "seccomp", Limits::default(), &script)?;
    let eperm = libc::EPERM.to_string();
    expect(
        seen.output.split_whitespace().count() == numbers.len()
            && seen.output.split_whitespace().all(|errno| errno == eperm),
        format!("every refused call answers EPERM, got {:?}", seen.output),
    )
}

pub(crate) fn landlock_denies(lane: &Lane) -> Result<(), Failed> {
    let script = "echo x > /workspace/x && echo workspace-ok; \
                  (echo y > /dev/landlock-probe) 2>/dev/null && echo dev-leaked || echo dev-denied; \
                  (echo z > /opt/x) 2>/dev/null && echo opt-leaked || echo opt-denied";
    let said = in_sandbox(lane, LANDLOCK, Limits::default(), script)?.output;
    expect(
        said.contains("workspace-ok") && said.contains("dev-denied") && said.contains("opt-denied"),
        format!("writes land only in the workspace, got {said:?}"),
    )
}

/// Nothing the sandbox does lands on the host as host root: the socket
/// directory takes no write at all once the sandbox is confined, and what it
/// writes in its workspace is owned by the unprivileged user it runs as.
pub(crate) fn plants_nothing(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine = lane.engine();
        let sandbox = engine
            .prepare(SandboxRequest {
                lease_id: PLANT,
                limits: Limits::default(),
            })
            .await?;
        let said = run(sandbox.executor(), shell(PLANT_SCRIPT)).await;
        let lease = lane.lease_dir(PLANT);
        let owner = fs::metadata(lease.join(OWNED_ON_HOST)).map(|meta| meta.uid());
        let run_dir: Vec<_> = fs::read_dir(lease.join("run"))?
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        sandbox.destroy().await?;
        let said = said?.output;
        expect(
            said.contains("sealed"),
            format!("the socket directory took a write: {said:?}"),
        )?;
        expect(
            said.contains(&afr_sandbox::bubblewrap::SANDBOX_UID.to_string()),
            format!("runs as its own user, got {said:?}"),
        )?;
        expect(
            run_dir.len() == 1,
            format!("only the socket is there: {run_dir:?}"),
        )?;
        expect(
            owner.as_ref().is_ok_and(|uid| *uid == SANDBOX_IDS.0),
            format!("the workspace file is owned by {owner:?} on the host"),
        )
    })
}
