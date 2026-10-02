//! The engine end to end on a fake host: no root, no bubblewrap, no kernel
//! features. The formatter and `mount` are `true`, the cgroup root a plain
//! directory, "bubblewrap" a script, and the executor is served in-process on
//! the socket the engine waits for — so every step of `prepare` and every
//! branch of teardown runs where an unprivileged test can reach it.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fake host it cannot build"
)]

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use afr_executor::{Ending, ProcessEvent, Spawn};
use tempfile::TempDir;
use tokio::task::JoinHandle;

use super::parts::Parts;
use super::{BubblewrapConfig, BubblewrapEngine};
use crate::engine::{Engine, Limits, SandboxRequest};
use crate::host::HostTools;
use crate::probe::ProbePaths;
use crate::toolbox::Toolbox;
use crate::warm_slots::WarmSlots;

/// A "bubblewrap" that stays up until it is killed.
/// The fake launcher that stays up, and the one that fails, by file name.
const SLEEPER_NAME: &str = "sleeper";
const FAILER_NAME: &str = "failer";
/// The fake host's security-module and seccomp-action files.
const LSM_FILE: &str = "lsm";
const ACTIONS_FILE: &str = "actions";
/// One processor core, in thousandths.
const ONE_CORE: u32 = 1_000;

const SLEEPER: &str = "#!/bin/sh\nexec sleep 30\n";
/// A "bubblewrap" that fails the way a refused mount does.
/// Its first line overruns the drain's line cap, which skips it and reads on,
/// and more lines follow than a refusal quotes, so the oldest are dropped.
const FAILER: &str = "#!/bin/sh\nprintf '%5000s\\n' '' >&2\nseq 1 30 >&2\n\
                      echo 'bwrap: Can not mount tmpfs: Operation not permitted' >&2\nexit 1\n";
/// What the failing one says, as a refusal quotes it.
const FAILER_REASON: &str = "Operation not permitted";
/// The program a formatter or `mount` is faked with.
const TRUE: &str = "/usr/bin/true";
/// A formatter that always fails.
const FALSE: &str = "/usr/bin/false";
/// A small workspace disk; the image is sparse and never formatted.
const LIMITS: Limits = Limits {
    memory_bytes: 1 << 28,
    cpu_millis: ONE_CORE,
    pids: 64,
    disk_bytes: 1 << 20,
};
/// How long a fake sandbox may take to answer.
const READY: Duration = Duration::from_secs(5);
/// How often the in-process executor looks for a new lease.
const POLL: Duration = Duration::from_millis(1);

/// The fake "bubblewrap" programs, written once per test process so no test
/// executes a file another thread is still writing.
fn script(body: &'static str) -> PathBuf {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [(SLEEPER_NAME, SLEEPER), (FAILER_NAME, FAILER)] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    });
    dir.path()
        .join(if body == SLEEPER { SLEEPER_NAME } else { FAILER_NAME })
}

/// A host that passes every probe, with `bwrap` as its launcher.
struct FakeHost {
    dir: TempDir,
    config: BubblewrapConfig,
}

impl FakeHost {
    fn new(bwrap: &Path) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("afr")
            .tempdir_in("/tmp")
            .unwrap();
        let base = dir.path();
        let cgroup_root = base.join("cgroup");
        fs::create_dir(&cgroup_root).unwrap();
        fs::write(
            cgroup_root.join("cgroup.subtree_control"),
            "cpu memory pids",
        )
        .unwrap();
        fs::write(base.join(LSM_FILE), "capability,landlock").unwrap();
        fs::write(base.join(ACTIONS_FILE), "errno allow").unwrap();
        fs::write(base.join("filesystems"), "\terofs\n").unwrap();
        let probe = ProbePaths {
            kvm: base.join("kvm"),
            filesystems: base.join("filesystems"),
            lsm: base.join(LSM_FILE),
            seccomp_actions: base.join(ACTIONS_FILE),
            cgroup_root: cgroup_root.clone(),
            bwrap: bwrap.to_owned(),
        };
        let config = BubblewrapConfig {
            tools: HostTools {
                bwrap: bwrap.to_owned(),
                mke2fs: PathBuf::from(TRUE),
                mount: PathBuf::from(TRUE),
            },
            probe,
            toolbox: Toolbox::at(base.join("toolbox")),
            cgroup_root,
            state_dir: base.join("leases"),
            entry: PathBuf::from(TRUE),
            entry_args: vec![OsString::from("sandbox")],
            ready_timeout: READY,
        };
        Self { dir, config }
    }

    fn engine(&self) -> BubblewrapEngine {
        BubblewrapEngine::new(self.config.clone()).unwrap()
    }

    fn lease_dir(&self, lease_id: &str) -> PathBuf {
        self.config.state_dir.join(lease_id)
    }
}

/// Serves the real executor, in-process, on every lease's socket as soon as
/// the engine makes its socket directory — what `agentsfleet-runner sandbox`
/// does inside a real sandbox.
fn serve_leases(state_dir: PathBuf) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut served = HashSet::new();
        loop {
            for entry in fs::read_dir(&state_dir).into_iter().flatten().flatten() {
                let lease = entry.path();
                let run = lease.join("run");
                if run.is_dir() && served.insert(lease.clone()) {
                    tokio::spawn(async move {
                        let socket = run.join(crate::bubblewrap::SOCKET_NAME);
                        afr_executor::serve(&socket, &lease.join("workspace")).await
                    });
                }
            }
            tokio::time::sleep(POLL).await;
        }
    })
}

fn request(lease_id: &str) -> SandboxRequest<'_> {
    SandboxRequest {
        lease_id,
        limits: LIMITS,
    }
}

#[tokio::test]
async fn test_a_lease_runs_through_the_engine_and_is_torn_down() {
    let host = FakeHost::new(&script(SLEEPER));
    let server = serve_leases(host.config.state_dir.clone());

    let sandbox = host.engine().prepare(request("lease-1")).await.unwrap();
    let mut process = sandbox
        .executor()
        .spawn(Spawn::program("/bin/echo").arg("hi"))
        .await
        .unwrap();
    let mut output = Vec::new();
    let ending = loop {
        match process.events.recv().await.unwrap() {
            ProcessEvent::Output { data, .. } => output.extend_from_slice(&data),
            ProcessEvent::Ended { ending, .. } => break ending,
        }
    };
    let joined = fs::read_to_string(host.config.cgroup_root.join("lease-1/cgroup.procs")).unwrap();
    let destroyed = sandbox.destroy().await;
    server.abort();

    assert_eq!(
        (ending, output.as_slice()),
        (Ending::Exited(0), b"hi\n".as_slice())
    );
    assert_eq!(
        joined, "0",
        "bubblewrap joined the lease's cgroup before it ran"
    );
    // A plain directory is no cgroup file system and nothing was mounted, so
    // removing both is refused; the lease's own directory goes regardless.
    destroyed.unwrap_err();
    assert!(!host.lease_dir("lease-1").exists());
}

#[tokio::test]
async fn test_a_sandbox_that_exits_refuses_the_lease_with_its_reason() {
    let host = FakeHost::new(&script(FAILER));
    let capture = Capture::install();

    let refused = host.engine().prepare(request("lease-2")).await.unwrap_err();

    let quoted = refused.to_string();
    assert!(quoted.contains(FAILER_REASON), "{quoted}");
    assert!(
        quoted.contains("\n30\n") && !quoted.contains("\n10\n"),
        "only the last lines: {quoted}"
    );
    assert_eq!(
        capture.only("sandbox_refused").field("lease_id"),
        Some("lease-2")
    );
    assert_eq!(
        capture.only("sandbox_teardown_failed").field("lease_id"),
        Some("lease-2")
    );
    assert!(!host.lease_dir("lease-2").exists());
}

#[tokio::test]
async fn test_a_sandbox_that_never_answers_refuses_the_lease() {
    let mut host = FakeHost::new(&script(SLEEPER));
    host.config.ready_timeout = Duration::from_millis(20);

    let refused = host.engine().prepare(request("lease-3")).await.unwrap_err();

    assert!(refused.to_string().contains("did not answer"), "{refused}");
    assert!(!host.lease_dir("lease-3").exists());
}

#[tokio::test]
async fn test_a_lease_never_inherits_a_directory_already_there() {
    let host = FakeHost::new(&script(SLEEPER));
    fs::create_dir_all(host.lease_dir("lease-4").join("workspace")).unwrap();

    host.engine().prepare(request("lease-4")).await.unwrap_err();

    assert!(
        host.lease_dir("lease-4").join("workspace").exists(),
        "left for the boot sweep"
    );
}

#[tokio::test]
async fn test_a_lease_identifier_that_escapes_is_refused_before_anything_is_built() {
    let host = FakeHost::new(&script(SLEEPER));

    let refused = host
        .engine()
        .prepare(request("../lease-5"))
        .await
        .unwrap_err();

    assert!(
        refused.to_string().contains("single path component"),
        "{refused}"
    );
    assert!(!host.dir.path().join("lease-5").exists());
}

#[tokio::test]
async fn test_a_workspace_disk_that_cannot_be_made_refuses_the_lease() {
    let mut host = FakeHost::new(&script(SLEEPER));
    host.config.tools.mke2fs = PathBuf::from(FALSE);

    let refused = host.engine().prepare(request("lease-6")).await.unwrap_err();

    assert!(refused.to_string().contains("mke2fs exited"), "{refused}");
    assert!(!host.lease_dir("lease-6").exists());
}

#[tokio::test]
async fn test_a_launcher_that_cannot_start_refuses_the_lease() {
    let mut host = FakeHost::new(&script(SLEEPER));
    host.config.tools.bwrap = host.dir.path().join("absent-bwrap");

    host.engine().prepare(request("lease-7")).await.unwrap_err();

    assert!(!host.lease_dir("lease-7").exists());
}

#[test]
fn test_a_host_without_landlock_refuses_every_lease() {
    let host = FakeHost::new(&script(SLEEPER));
    fs::write(&host.config.probe.lsm, "capability,yama").unwrap();

    let refused = BubblewrapEngine::new(host.config.clone()).unwrap_err();

    assert_eq!(refused.missing_mechanism(), Some("landlock"));
}

#[tokio::test]
async fn test_warm_slots_hand_out_a_bubblewrap_sandbox_started_ahead() {
    let host = FakeHost::new(&script(SLEEPER));
    let server = serve_leases(host.config.state_dir.clone());
    let capture = Capture::install();
    let slots = WarmSlots::start(Arc::new(host.engine()), 1, LIMITS);
    while !host
        .lease_dir("warm-1")
        .join("run")
        .join("executor.sock")
        .exists()
    {
        tokio::time::sleep(POLL).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let warm = slots.prepare(request("lease-8")).await.unwrap();
    warm.destroy().await.unwrap_err();
    slots.shutdown().await;
    server.abort();

    let started = capture
        .events()
        .into_iter()
        .find(|event| event.field("event") == Some("sandbox_start_ms"));
    assert_eq!(
        started
            .and_then(|event| event.field("start").map(str::to_owned))
            .as_deref(),
        Some("warm")
    );
    assert!(!host.lease_dir("warm-1").exists() && !host.lease_dir("warm-2").exists());
}

#[tokio::test]
async fn test_parts_with_no_sandbox_never_report_an_exit() {
    let dir = tempfile::tempdir().unwrap();
    let mut parts = Parts::new(dir.path().to_owned());

    let waited = tokio::time::timeout(Duration::from_millis(5), parts.exited()).await;

    waited.unwrap_err();
    parts.teardown().await.unwrap();
    assert!(
        !dir.path().exists(),
        "an empty lease's directory is all teardown removes"
    );
}

#[tokio::test]
async fn test_a_cgroup_that_cannot_be_made_refuses_the_lease() {
    let host = FakeHost::new(&script(SLEEPER));
    fs::create_dir(host.config.cgroup_root.join("lease-9")).unwrap();

    let refused = host.engine().prepare(request("lease-9")).await.unwrap_err();

    assert_eq!(
        refused.missing_mechanism(),
        None,
        "one lease failed, not the host"
    );
    assert!(!host.lease_dir("lease-9").exists());
}

/// What runs in the child between fork and exec, run here where it can be read.
#[test]
fn test_entering_a_cgroup_writes_this_process_into_it() {
    use std::os::fd::AsRawFd as _;
    let dir = tempfile::tempdir().unwrap();
    let procs = dir.path().join("cgroup.procs");
    let file = fs::File::create(&procs).unwrap();

    super::parts::enter(file.as_raw_fd()).unwrap();

    assert_eq!(fs::read_to_string(&procs).unwrap(), "0");
}
