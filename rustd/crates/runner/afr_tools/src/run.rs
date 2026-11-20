//! What every call of one run shares.
//!
//! Owned by the run and lent to every call through [`ToolContext`], so the
//! handlers stay shared across runs and hold no run's state. A run's child
//! loops call tools while their parent does, so each part a call changes sits
//! behind its own lock: one child's long call never waits on another's memory
//! write or credential mint.
//!
//! [`ToolContext`]: crate::ToolContext

use af_egress::Egress;
use afr_memory::MemoryBackend;
use tokio::sync::Mutex;

use guffbolt::tools::Sessions;

use crate::sandbox::Checkout;
use crate::verbs::RunVerbs;

/// One run's state, as its calls see it.
#[derive(Debug)]
pub struct Run<'run> {
    /// The run's id, as its guard was built for it.
    pub run_id: &'run str,
    /// The fleet's memory, behind the backend the fleet is bound to; held
    /// for one read or write, never across a call.
    pub memory: Mutex<Box<dyn MemoryBackend + 'run>>,
    /// The outbound guard every egress tool sends through: the run's policy,
    /// and the credentials it has minted; held to admit or mask, never across
    /// a send.
    pub egress: Mutex<Egress<'run>>,
    /// The processes the run's calls keep open across calls; the run's end
    /// closes whatever is left.
    pub sessions: Sessions,
    /// The repositories checked out in the run's workspace.
    pub checkouts: Vec<Checkout<'run>>,
    /// Whether the model's wire takes an image with a call's result; `image`
    /// refuses before any read when it does not.
    pub image_input: bool,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by this run.
    pub verbs: &'run dyn RunVerbs,
}

impl<'run> Run<'run> {
    /// A run whose calls read and write `memory`, send through `egress`, and
    /// reach `agentsfleetd` through `verbs`, with no session open yet and a
    /// wire that takes no image until told.
    #[must_use]
    pub fn new(
        memory: Box<dyn MemoryBackend + 'run>,
        egress: Egress<'run>,
        verbs: &'run dyn RunVerbs,
    ) -> Self {
        Self {
            run_id: egress.run_id(),
            memory: Mutex::new(memory),
            egress: Mutex::new(egress),
            sessions: Sessions::default(),
            checkouts: Vec::new(),
            image_input: false,
            verbs,
        }
    }

    /// The same run, whose workspace holds `checkouts`.
    #[must_use]
    pub fn with_checkouts(mut self, checkouts: Vec<Checkout<'run>>) -> Self {
        self.checkouts = checkouts;
        self
    }

    /// The same run, whose wire takes an image with a call's result, or
    /// not.
    #[must_use]
    pub const fn with_image_input(mut self, image_input: bool) -> Self {
        self.image_input = image_input;
        self
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Default for Run<'_> {
    /// A run with empty memory under the default backend, sending nothing
    /// and reaching no `agentsfleetd`.
    fn default() -> Self {
        Self::new(
            Box::new(afr_memory::Loaded::default()),
            af_egress::testing::closed(),
            &crate::verbs::CLOSED,
        )
    }
}
