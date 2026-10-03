//! What the handler suites share: one call, made the way the router makes it,
//! and a lease that sends through a fake.

use std::sync::Arc;
use std::sync::mpsc::Receiver;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_wire::policy::ExecutionPolicy;
use afr_egress::Egress;
use afr_egress::fixture::policy;
use afr_egress::testing::{CountingMint, RecordingTransport, Sent};
use afr_memory::Hydrated;

use crate::egress::SharedTransport;
use crate::lease::Lease;
use crate::runtime::{Tool, ToolContext, ToolOutput};

/// When every egress suite's clock starts.
pub(crate) const START: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);
/// What the fixture's mint answers for `github`.
pub(crate) const MINTED: &str = "ghs_minted_token";
/// A minted token's lifetime: one hour.
pub(crate) const HOUR: i64 = 3_600_000;

/// Calls `tool` with `arguments` from the supervisor, with `lease`'s state.
pub(crate) async fn call(
    tool: &dyn Tool,
    lease: &mut Lease<'_>,
    arguments: serde_json::Value,
) -> ToolOutput {
    tool.call(
        &arguments,
        ToolContext {
            executor: None,
            lease,
        },
    )
    .await
}

/// What an egress suite holds for one lease: the policy the guard admits
/// under, its clock, and a mint answering [`MINTED`].
pub(crate) struct Run {
    pub(crate) policy: ExecutionPolicy<'static>,
    pub(crate) clock: FixedClock,
    pub(crate) mint: CountingMint,
}

impl Run {
    /// The fixture policy, with `read_only` as given.
    pub(crate) fn new(read_only: bool) -> Self {
        let clock = FixedClock::at(START);
        Self {
            policy: policy(read_only),
            mint: CountingMint::answering(MINTED, HOUR, clock.clone()),
            clock,
        }
    }

    /// A lease sending through this run's guard.
    pub(crate) fn lease(&self) -> Lease<'_> {
        Lease::new(
            Box::new(Hydrated::default()),
            Egress::new(&self.policy, &self.mint, &self.clock),
        )
    }
}

/// A recording transport answering every request `status` with `body`, as
/// the handlers hold it, and where what it was handed arrives.
pub(crate) fn replying(status: u16, body: &str) -> (SharedTransport, Receiver<Sent>) {
    let (transport, sent) = RecordingTransport::replying(status, body);
    (Arc::new(transport), sent)
}
