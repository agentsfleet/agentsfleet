//! What every call of one lease shares.
//!
//! Owned by the run and lent to every call through [`ToolContext`], so the
//! handlers stay shared across leases and hold no lease's state. A run's child
//! loops call tools while their parent does, so each part a call changes sits
//! behind its own lock: one child's long call never waits on another's memory
//! write or credential mint.
//!
//! [`ToolContext`]: crate::ToolContext

use afr_egress::Egress;
use afr_memory::MemoryBackend;
use tokio::sync::Mutex;

use crate::sandbox::{Checkout, Sessions};
use crate::verbs::LeaseVerbs;

/// One lease's state, as its calls see it.
#[derive(Debug)]
pub struct Lease<'run> {
    /// The lease's id, as its guard was built for it.
    pub lease_id: &'run str,
    /// The fleet's memory, behind the backend the fleet is bound to; held
    /// for one read or write, never across a call.
    pub memory: Mutex<Box<dyn MemoryBackend + 'run>>,
    /// The outbound guard every egress tool sends through: the lease's policy,
    /// and the credentials it has minted; held to admit or mask, never across
    /// a send.
    pub egress: Mutex<Egress<'run>>,
    /// The processes the lease's calls keep open across calls; the run's end
    /// closes whatever is left.
    pub sessions: Sessions,
    /// The repositories checked out in the lease's workspace.
    pub checkouts: Vec<Checkout<'run>>,
    /// Whether the model's wire takes an image with a call's result; `image`
    /// refuses before any read when it does not.
    pub image_input: bool,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by this lease.
    pub verbs: &'run dyn LeaseVerbs,
}

impl<'run> Lease<'run> {
    /// A lease whose calls read and write `memory`, send through `egress`, and
    /// reach `agentsfleetd` through `verbs`, with no session open yet and a
    /// wire that takes no image until told.
    #[must_use]
    pub fn new(
        memory: Box<dyn MemoryBackend + 'run>,
        egress: Egress<'run>,
        verbs: &'run dyn LeaseVerbs,
    ) -> Self {
        Self {
            lease_id: egress.lease_id(),
            memory: Mutex::new(memory),
            egress: Mutex::new(egress),
            sessions: Sessions::default(),
            checkouts: Vec::new(),
            image_input: false,
            verbs,
        }
    }

    /// The same lease, whose workspace holds `checkouts`.
    #[must_use]
    pub fn with_checkouts(mut self, checkouts: Vec<Checkout<'run>>) -> Self {
        self.checkouts = checkouts;
        self
    }

    /// The same lease, whose wire takes an image with a call's result, or
    /// not.
    #[must_use]
    pub fn with_image_input(mut self, image_input: bool) -> Self {
        self.image_input = image_input;
        self
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Default for Lease<'_> {
    /// A lease with empty memory under the default backend, sending nothing
    /// and reaching no `agentsfleetd`.
    fn default() -> Self {
        Self::new(
            Box::new(afr_memory::Hydrated::default()),
            afr_egress::testing::closed(),
            &crate::verbs::CLOSED,
        )
    }
}
