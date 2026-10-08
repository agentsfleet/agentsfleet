//! The fake host every engine test builds on.
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

use tempfile::TempDir;
use tokio::task::JoinHandle;

use super::super::{BubblewrapConfig, BubblewrapEngine};
use crate::engine::{Limits, SandboxRequest};
use crate::host::HostTools;
use crate::probe::{HostProbe, Kvm, REQUIRED_CONTROLLERS};
use crate::toolbox::Toolbox;

/// The fake launchers, by file name.
const SLEEPER_NAME: &str = "sleeper";
const FAILER_NAME: &str = "failer";
const BRIEF_NAME: &str = "brief";
/// One processor core, in thousandths.
const ONE_CORE: u32 = 1_000;
/// The digest the fake host's toolbox is known by, and is configured for.
pub(super) const DIGEST: &str = "c4e5f5bb";
/// The lease directory's workspace image, which a refused unmount keeps.
pub(super) const IMAGE: &str = "workspace.img";

/// A "bubblewrap" that stays up until it is killed.
pub(super) const SLEEPER: Launcher = Launcher(SLEEPER_NAME);
/// A "bubblewrap" that fails the way a refused mount does.
pub(super) const FAILER: Launcher = Launcher(FAILER_NAME);
/// A "bubblewrap" that answers, then dies on its own a moment later.
pub(super) const BRIEF: Launcher = Launcher(BRIEF_NAME);
/// What the failing one says, as a refusal quotes it.
pub(super) const FAILER_REASON: &str = "Operation not permitted";
/// The program a formatter or `mount` is faked with.
const TRUE: &str = "/usr/bin/true";
/// A formatter that always fails.
pub(super) const FALSE: &str = "/usr/bin/false";
/// A small workspace disk; the image is sparse and never formatted.
pub(super) const LIMITS: Limits = Limits {
    memory_bytes: 1 << 28,
    cpu_millis: ONE_CORE,
    pids: 64,
    disk_bytes: 1 << 20,
};
/// How long a fake sandbox may take to answer.
const READY: Duration = Duration::from_secs(5);
/// How often the in-process executor looks for a new lease.
pub(super) const POLL: Duration = Duration::from_millis(1);

/// One fake launcher, by name.
#[derive(Debug, Clone, Copy)]
pub(super) struct Launcher(&'static str);

/// The scripts, written once per test process so no test executes a file
/// another thread is still writing. The failing one's first line overruns the
/// drain's line cap, which skips it and reads on, and more lines follow than a
/// refusal quotes, so the oldest are dropped.
fn scripts() -> &'static Path {
    static DIR: OnceLock<TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [
            (SLEEPER_NAME, "#!/bin/sh\nexec sleep 30\n"),
            (
                FAILER_NAME,
                "#!/bin/sh\nprintf '%5000s\\n' '' >&2\nseq 1 30 >&2\n\
                 echo 'bwrap: Can not mount tmpfs: Operation not permitted' >&2\nexit 1\n",
            ),
            (BRIEF_NAME, "#!/bin/sh\nexec sleep 0.3\n"),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        dir
    })
    .path()
}

/// A host that passes every probe, with `launcher` as its bubblewrap.
pub(super) struct FakeHost {
    pub(super) dir: TempDir,
    pub(super) config: BubblewrapConfig,
    /// What the host can enforce: everything, unless a test takes it away.
    pub(super) probe: HostProbe,
}

impl FakeHost {
    pub(super) fn new(launcher: Launcher) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("afr")
            .tempdir_in("/tmp")
            .unwrap();
        let base = dir.path();
        let bwrap = scripts().join(launcher.0);
        let cgroup_root = base.join("cgroup");
        fs::create_dir(&cgroup_root).unwrap();
        fs::write(
            cgroup_root.join("cgroup.subtree_control"),
            "cpu io memory pids",
        )
        .unwrap();
        let probe = HostProbe {
            landlock: true,
            seccomp: true,
            cgroup_controllers: REQUIRED_CONTROLLERS.map(str::to_owned).to_vec(),
            bubblewrap: true,
            kvm: Kvm::Absent,
            toolbox_filesystem: true,
            workspace_direct_io: None,
        };
        let config = BubblewrapConfig {
            tools: HostTools {
                bwrap,
                mke2fs: PathBuf::from(TRUE),
                mount: PathBuf::from(TRUE),
            },
            toolbox: Arc::new(Toolbox::at(base.join("toolbox"), DIGEST.to_owned())),
            toolbox_digest: DIGEST.to_owned(),
            cgroup_root,
            state_dir: base.join("leases"),
            entry: PathBuf::from(TRUE),
            entry_args: vec![OsString::from("sandbox")],
            sandbox_ids: (65_534, 65_534),
            log_level: Some(OsString::from("debug")),
            ready_timeout: READY,
        };
        Self { dir, config, probe }
    }

    pub(super) fn engine(&self) -> BubblewrapEngine {
        BubblewrapEngine::new(self.config.clone(), &self.probe).unwrap()
    }

    pub(super) fn lease_dir(&self, lease_id: &str) -> PathBuf {
        self.config.state_dir.join(lease_id)
    }

    /// The names of the lease directories that exist right now.
    pub(super) fn leases(&self) -> Vec<String> {
        fs::read_dir(&self.config.state_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }
}

/// Serves the real executor, in-process, on every lease's socket as soon as
/// the engine makes its socket directory — what `agentsfleet-runner sandbox`
/// does inside a real sandbox.
pub(super) fn serve_leases(state_dir: PathBuf) -> JoinHandle<()> {
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

pub(super) fn request(lease_id: &str) -> SandboxRequest<'_> {
    SandboxRequest {
        lease_id,
        limits: LIMITS,
    }
}
