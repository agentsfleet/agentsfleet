//! Each worker has its own address space, log and disposable local storage.

use super::{Checked, model::Model};
use crate::e2e::Scenario;
use std::{path::Path, process::Stdio};
use tokio::process::{Child, Command};

pub(super) struct Worker {
    pub(super) child: Child,
    pub(super) home: tempfile::TempDir,
}

impl Worker {
    pub(super) fn start_with(
        binary: &Path,
        run: &Scenario,
        token: &str,
        model: &Model,
    ) -> Checked<Self> {
        use afr_supervisor::config::{
            ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME, ENV_STREAM_CA, ENV_STREAM_URL,
        };
        // A failed cohort keeps diagnostics; kill() explicitly removes completed workers.
        let home = tempfile::Builder::new()
            .prefix("afr-recovery-")
            .disable_cleanup(true)
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

    pub(super) async fn kill(mut self) -> Checked<String> {
        self.child.kill().await?;
        let log = std::fs::read_to_string(self.home.path().join("runner.log"))?;
        self.home.close()?;
        Ok(log)
    }
}
