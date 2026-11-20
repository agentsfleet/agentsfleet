//! The services a runner-control handler is served through.

use afd_http::services::{
    FleetSchedules, FleetSteering, RunnerStreams, Shared, WorkspaceFleets, WorkspaceTurns,
};
use afd_runner::Runners;

use super::RunPlane;

/// The runner plane: a host speaking for itself, its runs' fenced verbs, and
/// the native session it opens.
pub trait RunnerPlane: Shared {
    /// The runner control plane's store.
    fn runners(&self) -> &Runners;

    /// What the run verbs act through. An associated type because the
    /// concrete plane holds a bus connection beside its stores, so a suite
    /// proving the router's refusal matrix must not need to construct one.
    type Runs: RunPlane;

    /// The run verbs' plane: assignments, gates, money, credentials. Separate
    /// from [`RunnerPlane::runners`]: the runner store answers "what is this
    /// host", and this answers "what may it run next".
    fn runs(&self) -> &Self::Runs;

    /// The native runner session owner, sharing the same authentication
    /// middleware.
    type RunnerStreams: RunnerStreams;

    /// Opens or attaches a lane after the common guard proves its principal.
    fn runner_streams(&self) -> &Self::RunnerStreams;

    /// Authoritative task lifecycle over the same fleet run and turn stores.
    fn tasks(&self) -> &dyn afd_fleet::task::TaskLifecycle;

    /// The fleets a runner's fleet may name.
    type Fleets: WorkspaceFleets;

    /// The workspace's fleets, as a run's fleet-to-fleet verbs read them.
    fn fleets(&self) -> &Self::Fleets;

    /// What a run's schedule verbs act through.
    type Schedules: FleetSchedules;

    /// What a schedule is read, written and fired through.
    fn schedules(&self) -> &Self::Schedules;

    /// What a fleet-to-fleet message acts through.
    type Steering: FleetSteering;

    /// A fleet's message ingress.
    fn steering(&self) -> &Self::Steering;

    /// What a run's reads of another fleet's turns act through.
    type Events: WorkspaceTurns;

    /// The narrative log a workspace and its fleets wrote.
    fn events(&self) -> &Self::Events;

    /// What an interim message is posted through: the outbound worker's Slack
    /// poster, whose API base is the seam a suite points at a loopback.
    fn interjector(&self) -> &afd_outbound::Interjector;
}
