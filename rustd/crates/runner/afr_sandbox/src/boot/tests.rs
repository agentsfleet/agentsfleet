#![expect(clippy::unwrap_used, reason = "test setup and assertions")]

use std::fs;
use std::sync::{Arc, Mutex};

use super::{Backend, HostPaths, Resources, Started, boot};
use crate::{Engine, HostProbe, Result, Sandbox, SandboxRequest};

#[derive(Debug)]
struct Recorder(Arc<Mutex<Vec<&'static str>>>);

impl Recorder {
    fn record(&self, event: &'static str) {
        self.0.lock().unwrap().push(event);
    }
}

#[derive(Debug)]
struct TestEngine(Recorder);

#[async_trait::async_trait]
impl Engine for TestEngine {
    async fn prepare(&self, _: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        Err(crate::error::refused("test engine has no executor"))
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        self.0.record("engine dropped");
    }
}

#[derive(Debug)]
struct TestResources(Recorder);

impl Resources for TestResources {
    fn close(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

impl Drop for TestResources {
    fn drop(&mut self) {
        self.0.record("resources dropped");
    }
}

const VALIDATE: &str = "validate";
const START: &str = "start";

struct TestBackend {
    events: Arc<Mutex<Vec<&'static str>>>,
    refuse: Option<&'static str>,
}

impl Backend for TestBackend {
    type Engine = TestEngine;

    fn validate(&self, _: &HostProbe) -> Result<()> {
        self.events.lock().unwrap().push(VALIDATE);
        if self.refuse == Some(VALIDATE) {
            return Err(crate::error::refused("test requirement"));
        }
        Ok(())
    }

    fn start(self, paths: &HostPaths, _: &HostProbe) -> Result<Started<Self::Engine>> {
        self.events.lock().unwrap().push(START);
        assert_eq!(
            fs::read_to_string(paths.cgroup_root.join(crate::SUBTREE_CONTROL)).unwrap(),
            "+cpu +io +memory +pids"
        );
        let resources = Box::new(TestResources(Recorder(Arc::clone(&self.events))));
        if self.refuse == Some(START) {
            return Err(crate::error::refused("test admission"));
        }
        Ok(Started {
            engine: TestEngine(Recorder(self.events)),
            resources,
        })
    }
}

fn fixture() -> (tempfile::TempDir, HostPaths, TestBackend) {
    let dir = tempfile::tempdir().unwrap();
    let paths = HostPaths {
        cgroup_root: dir.path().join("delegated"),
        state_dir: dir.path().join("leases"),
    };
    fs::create_dir(&paths.cgroup_root).unwrap();
    fs::create_dir(&paths.state_dir).unwrap();
    fs::write(
        paths.cgroup_root.join("cgroup.controllers"),
        "cpu io memory pids",
    )
    .unwrap();
    fs::write(paths.cgroup_root.join(crate::SUBTREE_CONTROL), "").unwrap();
    let backend = TestBackend {
        events: Arc::default(),
        refuse: None,
    };
    (dir, paths, backend)
}

#[test]
fn test_boot_uses_backend_policy_and_drops_engine_before_resources() {
    let (_dir, paths, backend) = fixture();
    let events = Arc::clone(&backend.events);
    let host = boot(backend, &paths).unwrap();
    assert_eq!(*events.lock().unwrap(), ["validate", "start"]);
    // Ordinary files retain the write command. This backend accepts these
    // facts without applying bubblewrap's capability policy.
    assert_eq!(
        host.probe.cgroup_controllers,
        ["+cpu", "+io", "+memory", "+pids"]
    );
    drop(host);
    assert_eq!(
        *events.lock().unwrap(),
        ["validate", "start", "engine dropped", "resources dropped"]
    );
}

#[test]
fn test_activation_failure_never_enters_the_backend() {
    let (_dir, paths, backend) = fixture();
    let events = Arc::clone(&backend.events);
    fs::remove_file(paths.cgroup_root.join(crate::SUBTREE_CONTROL)).unwrap();
    let error = boot(backend, &paths).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("refused a write to cgroup.subtree_control")
    );
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn test_backend_refusal_never_admits_resources_or_selects_a_fallback() {
    let (_dir, paths, mut backend) = fixture();
    let events = Arc::clone(&backend.events);
    backend.refuse = Some(VALIDATE);
    let error = boot(backend, &paths).unwrap_err();
    assert_eq!(error.missing_mechanism(), Some("test requirement"));
    assert_eq!(*events.lock().unwrap(), ["validate"]);
}

#[test]
fn test_failed_backend_admission_releases_its_resources() {
    let (_dir, paths, mut backend) = fixture();
    let events = Arc::clone(&backend.events);
    backend.refuse = Some(START);
    let error = boot(backend, &paths).unwrap_err();
    assert_eq!(error.missing_mechanism(), Some("test admission"));
    assert_eq!(
        *events.lock().unwrap(),
        ["validate", "start", "resources dropped"]
    );
}
