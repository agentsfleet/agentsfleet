//! The run clock a scenario that waits out a run span boots its daemon with.
//!
//! A run's lapse, a session's missed heartbeats and a host's offline derivation
//! are timed by the daemon's `[runner.run]` and `[runner.health]` keys, which
//! boot checks against one another in `afd_runner::timing::RunSettings`. A
//! scenario that waits one of them out boots with the shortest values those
//! checks admit, so it walks the production paths in less wall time. The runner
//! rounds on the heartbeat its welcome carries, so both ends keep this clock.
//!
//! Every other scenario keeps the production defaults. Its raw native sessions
//! and test `Session`s beat only when the test sends one, and at this cadence
//! the daemon closes a session after two silent seconds. A scenario booted here
//! drives the whole supervisor, whose own clock beats, or wants the silence.
//!
//! The run lifetime stops at 29 seconds. `af_core::timing`'s answer bound,
//! reconnect budget and renewal window set that floor, and none of them moves
//! with a deployment: the first two time the connection and the opening the
//! welcome arrives on, and the runner's process-cleanup margin holds the
//! renewal window up.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::{io::Write as _, time::Duration};

use afd_runner::timing::{MIN_HEARTBEAT_SECONDS, MIN_RUN_SECONDS};
use agentsfleetd::supervisor::Supervisor;

use crate::e2e::Scenario;

/// The shortest heartbeat a deployment may set: the runner's round, and half
/// the silence after which the daemon closes a session.
pub(crate) const HEARTBEAT: Duration = Duration::from_secs(MIN_HEARTBEAT_SECONDS);

/// The shortest run lifetime that leaves a complete renewal budget at that round.
pub(crate) const RUN_TTL: Duration = Duration::from_secs(MIN_RUN_SECONDS);

/// The shortest silence before a fleet read derives a host offline: one run lifetime.
const OFFLINE_AFTER: Duration = RUN_TTL;

/// The shortest healthy return that lifts quarantine: one second past a round.
const UNCORDON_AFTER: Duration = HEARTBEAT.saturating_add(Duration::from_secs(1));

/// [`crate::e2e::scenario`], its daemon booted on this clock.
pub(crate) async fn scenario(supervisor: &mut Supervisor) -> Scenario {
    scenario_with(supervisor, &[]).await
}

/// [`scenario`], with a suite's own knobs as [`crate::e2e::scenario_with`] takes them.
pub(crate) async fn scenario_with(supervisor: &mut Supervisor, extra: &[(&str, &str)]) -> Scenario {
    let file = clock_file();
    let path = file
        .path()
        .to_str()
        .expect("the temporary directory's path is UTF-8");
    let knobs: Vec<(&str, &str)> = extra
        .iter()
        .copied()
        .chain([(agentsfleetd::config::FILE_KNOB, path)])
        .collect();
    let scene = crate::e2e::scenario_with(supervisor, None, &knobs).await;
    // Boot read the file once, before any socket opened; nothing reads it again.
    file.close().expect("the clock file is removed");
    scene
}

fn clock_file() -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("a temporary clock file");
    write!(
        file,
        "[runner.run]\nttl_seconds = {}\n\n[runner.health]\n\
         heartbeat_interval_seconds = {}\noffline_after_seconds = {}\n\
         healthy_before_uncordon_seconds = {}\n",
        RUN_TTL.as_secs(),
        HEARTBEAT.as_secs(),
        OFFLINE_AFTER.as_secs(),
        UNCORDON_AFTER.as_secs(),
    )
    .expect("the clock file is written");
    file
}
