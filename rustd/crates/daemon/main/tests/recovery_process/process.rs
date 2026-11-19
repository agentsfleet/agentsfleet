//! Integration drill observations and graceful shutdown.
use super::{Checked, model::Model, worker::Worker};
use crate::e2e::Scenario;
use std::path::Path;
use tokio::process::Command;
impl Worker {
    pub(super) fn start(scene: &Scenario, token: &str, model: &Model) -> Checked<Self> {
        let binary = Path::new(env!("CARGO_BIN_EXE_agentsfleetd"))
            .with_file_name("agentsfleet-recovery-fixture");
        Self::start_with(&binary, scene, token, model)
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
