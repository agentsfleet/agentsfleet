//! An engine whose guest agent answers the suite's own commands as a faithful
//! engine would, with one flaw switched on at a time.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};

use af_guest_wire::{Ending, FileContent, Imported, Stream};
use af_wire::runner::{Isolation, SandboxEngine};
use afr_guest_client::{Events, GuestAgent, Process, ProcessId, Spawn};
use bytes::Bytes;
use libtest_mimic::Failed;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use self::sandbox::FakeSandbox;
use super::super::{Checkout, FarHost, files, network, run, secrets, walls};
use crate::engine::{
    Capabilities, Engine, Environment, Granted, Limits, ParkMode, ResourceFacts, Root, Sandbox,
    SandboxSpec, SnapshotRef, Sweep, Swept,
};
use crate::error::Result;

#[path = "fake_sandbox.rs"]
mod sandbox;

/// The far host every fake's verdicts are read against.
pub(super) const FAR: FarHost = FarHost {
    name: "far.test",
    listed: Ipv4Addr::new(10, 70, 0, 2),
    unlisted: Ipv4Addr::new(10, 70, 0, 3),
    port: 8443,
    greeting: "far",
};
/// The engine every fake says it is, and both modes it parks in.
const ENGINE: SandboxEngine = SandboxEngine::Firecracker;
const PARKS: &[ParkMode] = &[ParkMode::Pause, ParkMode::Stop];
/// What a full disk answers a writer, and the status `dd` then ends with.
const FULL: &str = "dd: error writing: No space left on device\n";
const FAILED: i32 = 1;

/// The one way a fake departs from a faithful engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Flaw {
    /// None: it answers as a faithful engine.
    Faithful,
    /// It declares `Pause` and answers `Unsupported` for it.
    PauseUnsupported,
    /// The metadata address answers an allowlisted sandbox.
    MetadataAnswers,
    /// A restore leaves the old environment's process running.
    RestoreKeepsAProcess,
    /// A restore brings a saved file back one byte off.
    RestoreChangesAByte,
}

/// The fake engine: its root, whether it offers snapshots, and its flaw.
#[derive(Debug)]
pub(super) struct Fake {
    pub(super) root: Root,
    pub(super) snapshots: bool,
    pub(super) flaw: Flaw,
}

impl Fake {
    /// A faithful engine on `root`, offering no snapshots.
    pub(super) const fn faithful(root: Root) -> Self {
        Self {
            root,
            snapshots: false,
            flaw: Flaw::Faithful,
        }
    }

    /// An engine on our toolbox, offering snapshots, with `flaw`.
    pub(super) fn snapshotting(flaw: Flaw) -> Self {
        Self {
            root: toolbox(),
            snapshots: true,
            flaw,
        }
    }

    /// An engine on our toolbox, offering no snapshots, with `flaw`.
    pub(super) fn flawed(flaw: Flaw) -> Self {
        Self {
            flaw,
            ..Self::faithful(toolbox())
        }
    }
}

/// Our toolbox, by a digest of its own.
pub(super) fn toolbox() -> Root {
    Root::Toolbox {
        digest: "sha256-fake-toolbox".into(),
    }
}

#[async_trait::async_trait]
impl Engine for Fake {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            engine: ENGINE,
            isolation: Isolation::HardwareVm,
            parks: PARKS,
            snapshots: self.snapshots,
            resources: ResourceFacts::ENFORCED,
        }
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        self.capabilities().grant(limits)
    }

    fn admission_bound(&self) -> Option<crate::AdmissionBound> {
        None
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        Ok(Box::new(FakeSandbox {
            id: spec.id.clone(),
            granted: spec.granted,
            environment: Environment {
                engine: ENGINE,
                root: self.root.clone(),
            },
            snapshots: self.snapshots,
            agent: FakeGuest {
                flaw: self.flaw,
                state: Arc::default(),
            },
        }))
    }

    async fn sweep(&self, _scope: Sweep<'_>) -> Result<Swept> {
        Ok(Swept::default())
    }
}

/// What one fake sandbox holds: its files, the tree last imported, whether
/// its background process runs, and the snapshots it took.
#[derive(Debug, Default)]
struct State {
    disk: Disk,
    running: bool,
    saved: Vec<(SnapshotRef, Disk)>,
}

/// What a snapshot saves.
#[derive(Debug, Default, Clone)]
struct Disk {
    files: BTreeMap<String, Vec<u8>>,
    tree: Vec<u8>,
}

/// The guest agent inside a fake sandbox.
#[derive(Debug)]
struct FakeGuest {
    flaw: Flaw,
    state: Arc<Mutex<State>>,
}

impl FakeGuest {
    #[expect(
        clippy::expect_used,
        reason = "test fake: a poisoned lock fails the test loudly"
    )]
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .expect("the fake's state is never poisoned")
    }

    /// What `script` says on each stream, and the status it ends with.
    fn answer(&self, script: &str) -> (Vec<(Stream, String)>, i32) {
        let out = |text: &str| vec![(Stream::Stdout, text.to_owned())];
        let state = &mut *self.state();
        match script {
            files::STREAMS => (
                vec![
                    (Stream::Stdout, "out".to_owned()),
                    (Stream::Stderr, "err".to_owned()),
                ],
                3,
            ),
            run::START_BACKGROUND => {
                state.running = true;
                (out(run::STARTED), 0)
            }
            run::BACKGROUND_ALIVE => (out(if state.running { run::ALIVE } else { run::GONE }), 0),
            run::REACH_OUT => (out(run::UNREACHABLE), 0),
            walls::WRITE_EVERYWHERE => (out("workspace-ok\ndev-denied\nopt-denied\n"), 0),
            walls::FILL_TMP | walls::FILL_SHARED_MEMORY => (out(FULL), FAILED),
            walls::ECHO_OK | walls::ALLOCATE_BESIDE_IT => (out(walls::OK), 0),
            network::RESOLVERS => (out("0"), 0),
            search if search == secrets::search() => {
                let holding: Vec<String> = state
                    .disk
                    .files
                    .iter()
                    .filter(|(_name, data)| contains(data, secrets::CANARY.as_bytes()))
                    .map(|(name, _data)| format!("/workspace/{name}"))
                    .collect();
                (out(&holding.join("\n")), 0)
            }
            dial => (out(&self.verdict(dial)), 0),
        }
    }

    /// What a dial prints: the greeting only where a faithful engine lets it
    /// through, and the metadata address too for the leaky flaw.
    fn verdict(&self, dial: &str) -> String {
        let Some((target, body)) = dial
            .strip_prefix("set -- ")
            .and_then(|rest| rest.split_once("; "))
        else {
            return format!("unknown command {dial:?}");
        };
        let host = target.split(' ').next().unwrap_or_default();
        let port = target.split(' ').nth(1).unwrap_or_default();
        let listed = host == FAR.name && port == FAR.port.to_string();
        let leaked = self.flaw == Flaw::MetadataAnswers && host == network::METADATA;
        match body {
            network::CONNECT if listed || leaked => FAR.greeting.to_owned(),
            network::CONNECT => format!("{} 111", network::ERRNO),
            network::DATAGRAM => network::NO_ANSWER.to_owned(),
            _other => format!("unknown command {dial:?}"),
        }
    }
}

/// Whether `haystack` holds `needle`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[async_trait::async_trait]
impl GuestAgent for FakeGuest {
    async fn spawn(&self, spawn: &Spawn) -> af_guest_wire::Result<Process> {
        let script = spawn.argv().get(2).map_or_default(String::as_str);
        let (said, status) = self.answer(script);
        let (feed, events) = Events::channel();
        for (stream, text) in said {
            feed.output(stream, Bytes::from(text));
        }
        feed.end(Ending::Exited(status), false);
        Ok(Process {
            id: ProcessId::new(1),
            events,
        })
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> af_guest_wire::Result<()> {
        Ok(())
    }

    async fn kill(&self, _process: ProcessId) -> af_guest_wire::Result<()> {
        Ok(())
    }

    async fn read_file(&self, path: &str, _max_bytes: u64) -> af_guest_wire::Result<FileContent> {
        let data = self.state().disk.files.get(path).cloned();
        let data = data.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        Ok(FileContent {
            data: Bytes::from(data),
            truncated: false,
        })
    }

    async fn write_file(&self, path: &str, data: Bytes) -> af_guest_wire::Result<()> {
        self.state()
            .disk
            .files
            .insert(path.to_owned(), data.to_vec());
        Ok(())
    }

    async fn import(
        &self,
        _at: &str,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> af_guest_wire::Result<Imported> {
        let mut read = Vec::new();
        tree.read_to_end(&mut read).await?;
        self.state().disk.tree = read;
        Ok(Imported::default())
    }

    async fn export(
        &self,
        _paths: &[&str],
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> af_guest_wire::Result<()> {
        let tree = self.state().disk.tree.clone();
        into.write_all(&tree).await?;
        Ok(())
    }
}

/// A checkout that lands one file and holds the token nowhere.
#[derive(Debug)]
pub(super) struct Landed;

#[async_trait::async_trait]
impl Checkout for Landed {
    async fn check_out(&self, sandbox: &dyn Sandbox, _token: &str) -> Result<(), Failed> {
        sandbox
            .agent()
            .write_file("README.md", Bytes::from_static(b"# widget\n"))
            .await?;
        Ok(())
    }
}
