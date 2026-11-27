//! Only the controlled model is reachable. A file/tool workspace is an invalid fixture.

use std::{io, net::SocketAddr, path::Path};

use af_core::env::ProcessEnv;
use af_wire::runner::{Isolation, SandboxEngine};
use afr_engine::{
    Cap, Capabilities, Engine, Granted, Limits, ResourceFacts, Sandbox, SandboxSpec, Sweep, Swept,
};
use tokio_util::sync::CancellationToken;

use super::Checked;

#[derive(Debug)]
struct ApiOnly;

#[async_trait::async_trait]
impl Engine for ApiOnly {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            engine: SandboxEngine::Unsandboxed,
            isolation: Isolation::Unconfined,
            parks: &[],
            snapshots: false,
            resources: ResourceFacts {
                memory: Cap::Absent,
                cpu: Cap::Absent,
                pids: Cap::Absent,
                disk: Cap::Absent,
            },
        }
    }

    fn grant(&self, _limits: Limits) -> afr_engine::Result<Granted> {
        Err(io::Error::other("API-only benchmark received a sandbox operation").into())
    }

    fn memory_bound(&self) -> Option<afr_engine::MemoryBound> {
        None
    }

    async fn prepare(&self, _spec: SandboxSpec<'_>) -> afr_engine::Result<Box<dyn Sandbox>> {
        Err(io::Error::other("API-only benchmark received a sandbox operation").into())
    }

    async fn sweep(&self, _scope: Sweep<'_>) -> afr_engine::Result<Swept> {
        Ok(Swept::default())
    }
}

pub(super) async fn run(address: SocketAddr, root: &Path) -> Checked {
    let (config, home) = afr_supervisor::boot(&ProcessEnv)?;
    let client = guffbolt_ai::testing::tls::client_from_file(root, address)?;
    let agent = afr_agent::Loop::new(
        afr_tools::Catalog::new(Vec::new()),
        af_providers::Connector::with_client(client),
    );
    let probe = stated_host();
    let stop = CancellationToken::new();
    let signal = stop.clone();
    let handler = tokio::spawn(async move {
        af_core::signal::shutdown().await;
        signal.cancel();
    });
    let result = afr_supervisor::run(
        &config,
        home,
        Box::new(ApiOnly),
        Box::new(agent),
        probe,
        stop,
    )
    .await;
    handler.abort();
    let _joined = handler.await;
    Ok(result?)
}

/// The host the debug engine runs on, as a stated probe: the engine confines
/// nothing, so no fact the host could report changes what it builds.
fn stated_host() -> afr_engine::HostProbe {
    afr_engine::HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: afr_engine::REQUIRED_CONTROLLERS.map(str::to_owned).to_vec(),
        kvm: af_wire::runner::Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
        egress: false,
    }
}
