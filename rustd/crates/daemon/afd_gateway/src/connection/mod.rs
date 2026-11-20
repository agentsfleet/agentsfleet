//! One bounded owner joins the four streams, domain operations and every child task.

mod assign;
mod assignments;
pub(crate) mod cache;
mod commands;
mod context;
mod dispatch;
mod events;
pub(crate) mod io;
pub(crate) mod liveness;
pub(crate) mod opening;
pub(crate) mod requests;
mod runtime;
mod streams;
mod supervisor;

use std::{num::NonZeroU64, sync::Arc};

use af_core::id::Uuid7;
use af_wire::{protocol::Down, run::AcceptedKinds};
use afd_auth::principal::Runner;
use afd_runner::capacity::authority::{Authority, Prepared};
use afd_sse::StreamPermit;
use parking_lot::Mutex;
use tokio::{
    sync::{Notify, mpsc},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::{ConnectionContext, Gateway};

pub(crate) use assignments::{AssignMode, AssignOutcome, PassRequest};
pub(crate) use io::{StreamAttachment, StreamAttachmentSender};
use io::{StreamIo, StreamWriters};

use af_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};

type Message<'a> = Down<'a, afd_setup::types::Candidate, Definition, ConfigOp, InstructionEdit>;
type Request<'a> = af_wire::protocol::Up<'a, Definition, ConfigOp, InstructionEdit>;

/// The scheduler reads only eligibility, available workers and locality preferences.
#[derive(Debug, Clone)]
pub(crate) struct SchedulingState {
    pub(crate) ready: bool,
    pub(crate) degraded: bool,
    pub(crate) credits: usize,
    pub(crate) assignments_sent: u64,
    pub(crate) assignments_seen: u64,
    pub(crate) ready_revision: Option<NonZeroU64>,
    pub(crate) held: Vec<Uuid7>,
    pub(crate) last_assignment: Instant,
    pub(crate) recovery_pending: Option<afd_bus::wake::Recovery>,
}

impl SchedulingState {
    fn recovered(&mut self, observed: Option<afd_bus::wake::Recovery>) {
        if self.recovery_pending == observed {
            self.recovery_pending = None;
        }
    }

    fn availability(
        &mut self,
        credits: usize,
        revision: NonZeroU64,
        observed_assignments: u64,
    ) -> crate::Result<bool> {
        if self
            .ready_revision
            .is_some_and(|current| revision <= current)
        {
            return Ok(false);
        }
        if observed_assignments < self.assignments_seen
            || observed_assignments > self.assignments_sent
        {
            return Err(crate::error::ErrorKind::Protocol.into());
        }
        let previous = if self.ready { self.credits } else { 0 };
        let unseen = self.assignments_sent - observed_assignments;
        // The comparison proves any subsequent narrowing fits the supplied usize capacity.
        self.credits = if unseen >= credits as u64 {
            0
        } else {
            credits - usize::try_from(unseen)?
        };
        self.assignments_seen = observed_assignments;
        self.ready_revision = Some(revision);
        self.ready = true;
        Ok(self.credits > previous)
    }

    fn issued(&mut self) -> crate::Result<()> {
        self.assignments_sent = self
            .assignments_sent
            .checked_add(1)
            .ok_or(crate::error::ErrorKind::Protocol)?;
        self.credits = self.credits.saturating_sub(1);
        self.last_assignment = Instant::now();
        Ok(())
    }
}

/// Shared notifications carry no authority; the connection retains the database-issued proof.
#[derive(Debug)]
pub(crate) struct ConnectionHandle {
    pub(crate) runner: Uuid7,
    pub(crate) connection_id: Uuid7,
    pub(crate) snapshot: Mutex<SchedulingState>,
    pub(crate) stop: CancellationToken,
    pub(crate) drain: CancellationToken,
    pub(crate) liveness: liveness::Liveness,
    pub(crate) superseded: CancellationToken,
    pub(crate) assignments: Notify,
    pub(crate) passes: mpsc::Sender<PassRequest>,
    pub(crate) recovery_scan: Mutex<afd_fleet::run::RecoveryScan>,
    pub(crate) rechecks: Notify,
    pub(crate) commands: mpsc::Sender<afd_bus::Command>,
}

impl ConnectionHandle {
    fn apply_policy(&self, degraded: bool) {
        let recovered = {
            let mut state = self.snapshot.lock();
            let recovered = state.degraded && !degraded;
            state.degraded = degraded;
            recovered
        };
        if recovered {
            self.request_assignment();
        }
    }

    /// A full command queue becomes a complete durable recheck; it never silently loses authority changes.
    fn command(&self, command: afd_bus::Command) {
        if self.commands.try_send(command).is_err() {
            self.rechecks.notify_one();
        }
    }

    pub(crate) fn request_assignment(&self) {
        self.assignments.notify_one();
    }

    pub(crate) fn supersede(&self) {
        self.superseded.cancel();
    }

    pub(crate) fn is_superseded(&self) -> bool {
        self.superseded.is_cancelled()
    }
}

/// Parsed opening data owns its identifiers after the charged input frame drops.
struct Hello {
    request_id: NonZeroU64,
    accepted: AcceptedKinds,
    /// The protocol revision the control stream named.
    revision: af_wire::protocol::Revision,
    running: Vec<Uuid7>,
    credits: usize,
    unconfirmed: Option<Uuid7>,
}

/// This state owns a proposal; it cannot issue work before confirmation.
pub(crate) struct ConnectionOpening {
    gateway: Gateway,
    context: Arc<ConnectionContext>,
    runner: Runner,
    hello: Hello,
    prepared: Prepared,
    io: StreamIo,
    permit: StreamPermit,
    deadline: Instant,
}

/// Ownership is committed before this value exists; it still owns the one admission permit.
struct ConfirmedConnection {
    gateway: Gateway,
    context: Arc<ConnectionContext>,
    runner: Runner,
    hello: Hello,
    authority: Authority,
    cache: cache::RunCache,
    attachments: mpsc::Receiver<StreamAttachment>,
    io: StreamIo,
    permit: StreamPermit,
    deadline: Instant,
}

/// Bound service clones reuse the same pools, bus and transaction-level owner checks.
pub(crate) struct ConnectionServices {
    gateway: Gateway,
    pub(crate) handle: Arc<ConnectionHandle>,
    authority: Authority,
    runs: afd_fleet::run::RunService,
    drafts: afd_setup::Drafts,
    models: afd_setup::ModelCalls,
    creations: afd_setup::Creations,
    cache: cache::RunCache,
    output: StreamWriters,
    accepted: AcceptedKinds,
    /// Whether this runner's revision may be assigned a draft-owned run.
    draft_runs: bool,
    checkpoint_verifications: tokio::sync::Semaphore,
    /// Renewal frames this connection has answered, for
    /// [`afd_fleet::run::renew::funds_due`]'s cadence.
    renewal_ticks: std::sync::atomic::AtomicU64,
}

pub(crate) async fn open(
    gateway: &Gateway,
    runner: Runner,
    request: axum::extract::Request,
) -> crate::Result<axum::response::Response> {
    opening::open(gateway, runner, request).await
}

#[cfg(test)]
mod credits_tests;

#[cfg(test)]
impl ConnectionHandle {
    pub(crate) fn fixture(runner: Uuid7, connection_id: Uuid7) -> Arc<Self> {
        Self::fixture_with_passes(runner, connection_id).0
    }

    pub(crate) fn fixture_with_passes(
        runner: Uuid7,
        connection_id: Uuid7,
    ) -> (Arc<Self>, mpsc::Receiver<PassRequest>) {
        let (commands, _) = mpsc::channel(1);
        let (passes, requests) = mpsc::channel(1);
        let connection = Arc::new(Self {
            runner,
            connection_id,
            snapshot: Mutex::new(SchedulingState {
                ready: true,
                degraded: false,
                credits: 1,
                assignments_sent: 0,
                assignments_seen: 0,
                ready_revision: None,
                held: Vec::new(),
                last_assignment: Instant::now(),
                recovery_pending: None,
            }),
            stop: CancellationToken::new(),
            drain: CancellationToken::new(),
            liveness: liveness::Liveness::default(),
            superseded: CancellationToken::new(),
            assignments: Notify::new(),
            passes,
            recovery_scan: Mutex::default(),
            rechecks: Notify::new(),
            commands,
        });
        (connection, requests)
    }
}
