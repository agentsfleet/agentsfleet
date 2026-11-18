//! Each worker has its own address space, log and disposable local storage.

use super::{Checked, model::Model};
use crate::e2e::Scenario;
use std::{path::Path, process::Stdio};
use tokio::process::{Child, Command};

pub(super) struct Worker {
    child: Child,
    home: tempfile::TempDir,
}

impl Worker {
    pub(super) fn start(run: &Scenario, token: &str, model: &Model) -> Checked<Self> {
        use afr_supervisor::config::{
            ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME, ENV_STREAM_CA, ENV_STREAM_URL,
        };
        let binary = Path::new(env!("CARGO_BIN_EXE_agentsfleetd"))
            .with_file_name("agentsfleet-recovery-fixture");
        let home = tempfile::Builder::new()
            .prefix("afr-recovery-")
            .tempdir_in("/tmp")?;
        let log = std::fs::File::create(home.path().join("runner.log"))?;
        let child = Command::new(binary)
            .arg("--model-address")
            .arg(model.address.to_string())
            .arg("--model-root")
            .arg(model.root())
            .env(ENV_API_URL, &run.base)
            .env(ENV_RUNNER_TOKEN, token)
            .env(ENV_STORAGE_HOME, home.path())
            .env(ENV_STREAM_CA, crate::support::runner_stream::root_file())
            .env(
                ENV_STREAM_URL,
                format!(
                    "https://localhost:{}",
                    run.booted.runner_stream_address.port()
                ),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log))
            .kill_on_drop(true)
            .spawn()?;
        Ok(Self { child, home })
    }

    pub(super) async fn next(&mut self, model: &mut Model) -> Checked<super::model::Asked> {
        let result = tokio::select! {
            request = model.next() => request,
            status = self.child.wait() => {
                let status = status?;
                Err(std::io::Error::other(format!("runner exited before the next model request: {status}")).into())
            }
        };
        if result.is_err() {
            let log = std::fs::read_to_string(self.home.path().join("runner.log"))?;
            tracing::error!(event = "recovery_fixture_process_failed", diagnostic = log);
        }
        result
    }

    pub(super) async fn kill(mut self) -> Checked<String> {
        self.child.kill().await?;
        let log = std::fs::read_to_string(self.home.path().join("runner.log"))?;
        self.home.close()?;
        Ok(log)
    }

    pub(super) async fn stop(mut self) -> Checked<String> {
        let pid = self
            .child
            .id()
            .ok_or_else(|| std::io::Error::other("runner exited early"))?;
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .await?;
        assert!(status.success());
        let status =
            tokio::time::timeout(std::time::Duration::from_secs(60), self.child.wait()).await??;
        let log = std::fs::read_to_string(self.home.path().join("runner.log"))?;
        assert!(status.success(), "runner shutdown: {log}");
        Ok(log)
    }
}
