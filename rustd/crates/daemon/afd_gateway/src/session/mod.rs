//! One bounded owner joins the four streams, domain operations and every child task.

mod cache;
mod commands;
mod context;
mod dispatch;
mod events;
mod io;
mod offer;
mod offers;
mod opening;
mod requests;
mod runtime;
mod streams;
mod supervisor;

use std::{num::NonZeroU64, sync::Arc};

use af_core::id::Uuid7;
use af_wire::{lease::AcceptedKinds, protocol::Down};
use afd_auth::principal::Runner;
use afd_runner::capacity::authority::{Authority, Prepared};
use afd_sse::StreamPermit;
use parking_lot::Mutex;
use tokio::{
    sync::{Notify, mpsc, oneshot},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::{ConnectionContext, Gateway};

pub(crate) use io::{StreamAttachment, StreamAttachmentSender};
use io::{StreamIo, StreamWriters};

use af_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};

type Message<'a> = Down<'a, afd_authoring::types::Candidate, Definition, ConfigOp, InstructionEdit>;
type Request<'a> = af_wire::protocol::Up<'a, Definition, ConfigOp, InstructionEdit>;

/// The scheduler reads only eligibility, available workers and locality preferences.
#[derive(Debug, Clone)]
pub(crate) struct SchedulingState {
    pub(crate) ready: bool,
    pub(crate) degraded: bool,
    pub(crate) credits: usize,
    offers_sent: u64,
    offers_seen: u64,
    ready_revision: Option<NonZeroU64>,
    pub(crate) held: Vec<Uuid7>,
    pub(crate) last_offer: Instant,
}

impl SchedulingState {
    fn availability(
        &mut self,
        credits: usize,
        revision: NonZeroU64,
        observed_offers: u64,
    ) -> crate::Result<bool> {
        if self
            .ready_revision
            .is_some_and(|current| revision <= current)
        {
            return Ok(false);
        }
        if observed_offers < self.offers_seen || observed_offers > self.offers_sent {
            return Err(crate::error::ErrorKind::Protocol.into());
        }
        let previous = if self.ready { self.credits } else { 0 };
        let unseen = self.offers_sent - observed_offers;
        // The comparison proves any subsequent narrowing fits the supplied usize capacity.
        self.credits = if unseen >= credits as u64 {
            0
        } else {
            credits - usize::try_from(unseen)?
        };
        self.offers_seen = observed_offers;
        self.ready_revision = Some(revision);
        self.ready = true;
        Ok(self.credits > previous)
    }

    fn issued(&mut self) -> crate::Result<()> {
        self.offers_sent = self
            .offers_sent
            .checked_add(1)
            .ok_or(crate::error::ErrorKind::Protocol)?;
        self.credits = self.credits.saturating_sub(1);
        self.last_offer = Instant::now();
        Ok(())
    }
}

/// Shared notifications carry no authority; the session retains the database-issued proof.
#[derive(Debug)]
pub(crate) struct SessionHandle {
    pub(crate) runner: Uuid7,
    pub(crate) connection: Uuid7,
    pub(crate) snapshot: Mutex<SchedulingState>,
    pub(crate) stop: CancellationToken,
    pub(crate) drain: CancellationToken,
    superseded: CancellationToken,
    offers: Notify,
    claims: mpsc::Sender<oneshot::Sender<bool>>,
    rechecks: Notify,
    commands: mpsc::Sender<afd_bus::Command>,
}

impl SessionHandle {
    /// A full command queue becomes a complete durable recheck; it never silently loses authority changes.
    fn command(&self, command: afd_bus::Command) {
        if self.commands.try_send(command).is_err() {
            self.rechecks.notify_one();
        }
    }

    pub(crate) fn request_offer(&self) {
        self.offers.notify_one();
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
    running: Vec<Uuid7>,
    credits: usize,
    unconfirmed: Option<Uuid7>,
}

/// This state owns a proposal; it cannot issue work before confirmation.
pub(crate) struct SessionOpening {
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
struct ConfirmedSession {
    gateway: Gateway,
    context: Arc<ConnectionContext>,
    runner: Runner,
    hello: Hello,
    authority: Authority,
    cache: cache::LeaseCache,
    attachments: mpsc::Receiver<StreamAttachment>,
    io: StreamIo,
    permit: StreamPermit,
    deadline: Instant,
}

/// Bound service clones reuse the same pools, bus and transaction-level owner checks.
struct SessionServices {
    gateway: Gateway,
    handle: Arc<SessionHandle>,
    authority: Authority,
    leases: afd_fleet::lease::LeaseService,
    drafts: afd_authoring::Drafts,
    models: afd_authoring::ModelCalls,
    creations: afd_authoring::Creations,
    cache: cache::LeaseCache,
    output: StreamWriters,
    accepted: AcceptedKinds,
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
impl SessionHandle {
    pub(crate) fn fixture(runner: Uuid7, connection: Uuid7) -> Arc<Self> {
        Self::fixture_with_claims(runner, connection).0
    }

    pub(crate) fn fixture_with_claims(
        runner: Uuid7,
        connection: Uuid7,
    ) -> (Arc<Self>, mpsc::Receiver<oneshot::Sender<bool>>) {
        let (commands, _) = mpsc::channel(1);
        let (claims, requests) = mpsc::channel(1);
        let session = Arc::new(Self {
            runner,
            connection,
            snapshot: Mutex::new(SchedulingState {
                ready: true,
                degraded: false,
                credits: 1,
                offers_sent: 0,
                offers_seen: 0,
                ready_revision: None,
                held: Vec::new(),
                last_offer: Instant::now(),
            }),
            stop: CancellationToken::new(),
            drain: CancellationToken::new(),
            superseded: CancellationToken::new(),
            offers: Notify::new(),
            claims,
            rechecks: Notify::new(),
            commands,
        });
        (session, requests)
    }
}
