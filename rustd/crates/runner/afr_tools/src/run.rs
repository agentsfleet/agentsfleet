//! What every call of one run shares.
//!
//! Owned by the run, lent to every call through [`ToolContext`], and borrowed
//! by the backends the run's own tools are built over ([`crate::Backends`]),
//! so the hosted handlers stay shared across runs and hold no run's state. A
//! run's child loops call tools while their parent does, so each part a call
//! changes sits behind its own lock: one child's long call never waits on
//! another's memory write or credential mint.
//!
//! [`ToolContext`]: crate::ToolContext

use std::collections::HashMap;
use std::sync::{Mutex as StdMutex, PoisonError};

use af_egress::Egress;
use afr_memory::Loaded;
use tokio::sync::Mutex;

use guffbolt::tools::{Limits, Sessions};

use crate::generic::RunMemory;
use crate::sandbox::Checkout;
use crate::verbs::RunVerbs;

/// One run's state, as its calls see it.
#[derive(Debug)]
pub struct Run<'run> {
    /// The run's id, as its guard was built for it.
    pub run_id: &'run str,
    /// The fleet's memory, which the memory tools are built over.
    pub memory: RunMemory<'run>,
    /// The outbound guard every egress tool sends through: the run's policy,
    /// and the credentials it has minted; held to admit or mask, never across
    /// a send.
    pub egress: Mutex<Egress<'run>>,
    /// The processes the run's calls keep open across calls; the run's end
    /// closes whatever is left.
    pub sessions: Sessions,
    /// The bounds the run's built-in tools hold its calls to.
    pub limits: Limits,
    /// The repositories checked out in the run's workspace.
    pub checkouts: Vec<Checkout<'run>>,
    /// Whether the model's wire takes an image with a call's result; `image`
    /// refuses before any read when it does not.
    pub image_input: bool,
    /// The `agentsfleetd` verbs the schedule and message tools reach, fenced
    /// by this run.
    pub verbs: &'run dyn RunVerbs,
    /// Each open call's number in the run, which an effect names its item by.
    pub calls: CallNumbers,
}

/// Each open call's number in its run, by the provider's id for the call.
///
/// The run's ledger numbers a call when it opens and forgets it when it
/// ends; a message, schedule or cron call sends its number so `agentsfleetd`
/// keys the effect by the call's item, and a retry lands once.
#[derive(Debug, Default)]
pub struct CallNumbers(StdMutex<HashMap<String, u64>>);

impl CallNumbers {
    /// Call `call_id` opened as the run's `number`th.
    pub fn open(&self, call_id: &str, number: u64) {
        self.held().insert(call_id.to_owned(), number);
    }

    /// Call `call_id` ended.
    pub fn close(&self, call_id: &str) {
        self.held().remove(call_id);
    }

    /// The number of open call `call_id`, if one is open under it.
    #[must_use]
    pub fn of(&self, call_id: &str) -> Option<u64> {
        self.held().get(call_id).copied()
    }

    /// The map, whatever a panicking holder left: each entry is written whole.
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<String, u64>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<'run> Run<'run> {
    /// A run whose calls read and write `memory`, send through `egress`, and
    /// reach `agentsfleetd` through `verbs`, with no session open yet and a
    /// wire that takes no image until told.
    #[must_use]
    pub fn new(memory: Loaded<'run>, egress: Egress<'run>, verbs: &'run dyn RunVerbs) -> Self {
        Self {
            run_id: egress.run_id(),
            memory: RunMemory::new(memory),
            egress: Mutex::new(egress),
            sessions: Sessions::default(),
            limits: Limits::DEFAULT,
            checkouts: Vec::new(),
            image_input: false,
            verbs,
            calls: CallNumbers::default(),
        }
    }

    /// The same run, whose workspace holds `checkouts`.
    #[must_use]
    pub fn with_checkouts(mut self, checkouts: Vec<Checkout<'run>>) -> Self {
        self.checkouts = checkouts;
        self
    }

    /// The same run, its built-in tools holding its calls to `limits` and
    /// keeping at most `max_sessions` sessions open. Set before any call
    /// opens a session: the registry is replaced.
    #[must_use]
    pub fn with_limits(mut self, limits: Limits, max_sessions: usize) -> Self {
        self.limits = limits;
        self.sessions = Sessions::new(max_sessions);
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
            Loaded::default(),
            af_egress::testing::closed(),
            &crate::verbs::CLOSED,
        )
    }
}
