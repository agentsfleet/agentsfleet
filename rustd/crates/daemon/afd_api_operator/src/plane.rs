//! The services an operator handler is served through.

use afd_admin::{Models, PlatformKeys};
use afd_fleet_ops::RunnerRunHistory;
use afd_http::services::Shared;
use afd_runner::Runners;

/// The operator plane: runner inventory and mutations, run history, and the
/// platform's priced catalogue and default keys.
///
/// Every store here is concrete: each holds a Postgres pool and nothing else,
/// so `afd_db::Db::unreachable` is the whole of the seam a suite needs.
pub trait OperatorPlane: Shared {
    /// The runner control plane's store.
    fn runners(&self) -> &Runners;

    /// The process-wide publisher a runner mutation's commands go through,
    /// retained even when live subscriptions are unavailable.
    fn runner_commands(&self) -> &afd_bus::Handle;

    /// Read-only cross-table projections for fleet operators.
    fn runner_run_history(&self) -> &RunnerRunHistory;

    /// Priced-model catalogue administration: the admin plane's write of the
    /// rows a tenant reads through its own catalogue, gated behind platform
    /// scopes so a tenant route cannot reach a mutation.
    fn models(&self) -> &Models;

    /// Reveal-free platform-default administration.
    fn platform_keys(&self) -> &PlatformKeys;
}
