//! Durable mutations keep their response even when their disposable notification is lost.

use af_core::id::Uuid7;
use afd_bus::{Command, Subject};

use crate::services::Services;

pub(super) async fn changed<D: Services>(services: &D, runner: &Uuid7) {
    let subject = Subject::RunnerCommand(runner.clone());
    let outcome = Command::Recheck
        .publish(services.runner_commands(), runner)
        .await;
    afd_bus::observe_notification(&subject, outcome);
}
