//! What a tenant process inside a guest may not do: hold a capability or a
//! socket, make a refused system call, or write where the guest's own files
//! live; and what it writes lands in an image only its jail's user opens.

use std::fs;
use std::os::unix::fs::MetadataExt as _;

use afg_confine::{REFUSED_SYSCALLS, X32_SYSCALL_BIT};
use afr_engine::{Engine as _, Limits};
use libtest_mimic::Failed;

use crate::lane::{Lane, WORKSPACE_IMAGE, id_of, isolated};
use crate::run::{expect, in_sandbox, run, runtime, shell};

/// The sandbox the planting trial runs in.
const PLANT: &str = "plant";
/// Tries to leave a set-user-ID file in the guest's `/run`, then says who it
/// runs as and writes one file where it may.
const PLANT_SCRIPT: &str = "(echo x > /run/planted && chmod u+s /run/planted) \
     2>/dev/null && echo planted || echo sealed; id -u; echo y > /workspace/owned";
/// The user identifier of root, which no tenant process runs as.
const ROOT_UID: u32 = 0;
/// What a socket descriptor's link reads as, before its inode.
const SOCKET: &str = "socket:";
/// Lists the descriptors a tenant process holds.
const LIST_DESCRIPTORS: &str = "ls -l /proc/self/fd";

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

/// A tenant process holds no capability.
pub(crate) fn no_capabilities(lane: &Lane) -> Result<(), Failed> {
    let status = in_sandbox(lane, "caps", Limits::default(), "cat /proc/self/status")?;
    Ok(afg_confine::capabilities_dropped(&status.output)?)
}

/// Every system call the filter lists answers `EPERM` to a tenant process.
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

/// A process the guest agent starts holds no socket: the guest agent's vsock link
/// stays the guest agent's.
pub(crate) fn tenant_holds_no_agent_socket(lane: &Lane) -> Result<(), Failed> {
    let listed = in_sandbox(lane, "link-tenant", Limits::default(), LIST_DESCRIPTORS)?.output;
    expect(
        !listed.contains(SOCKET),
        format!("a tenant process holds a socket:\n{listed}"),
    )
}

/// Nothing a tenant does lands as root: the guest's `/run` takes no
/// set-user-ID file, the tenant runs as its own user, and the image its
/// workspace file lands in is owned on the host by its jail's user, not root.
pub(crate) fn plants_nothing(lane: &Lane) -> Result<(), Failed> {
    runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of(PLANT);
        let sandbox = engine.prepare(isolated(&engine, &id)?).await?;
        let said = run(sandbox.agent(), shell(PLANT_SCRIPT)).await;
        let image = lane.run_dir(PLANT).join(WORKSPACE_IMAGE);
        let owner = fs::metadata(image).map(|meta| meta.uid());
        sandbox.destroy().await?;
        let said = said?.output;
        expect(
            said.contains("sealed"),
            format!("/run took a write: {said:?}"),
        )?;
        let uid = said.lines().nth(1).and_then(|uid| uid.trim().parse().ok());
        expect(
            uid.is_some_and(|uid: u32| uid != ROOT_UID),
            format!("runs as its own user, got {said:?}"),
        )?;
        expect(
            owner.as_ref().is_ok_and(|uid| *uid != ROOT_UID),
            format!("the workspace image is owned by {owner:?} on the host"),
        )
    })
}
