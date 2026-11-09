//! One bounded owner joins the four streams, domain operations and every child task.

mod cache;
mod commands;
mod context;
mod dispatch;
mod io;
mod offer;
mod opening;
mod events;
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
use tokio::{sync::{mpsc, watch, Notify}, time::Instant};
use tokio_util::sync::CancellationToken;

use crate::{ConnectionContext, Gateway};

pub(crate) use io::{StreamAttachment, StreamAttachmentSender};
use io::{StreamIo, StreamWriters};

type Message<'a> = Down<'a, afd_authoring::types::Candidate>;

/// The scheduler reads only eligibility, available workers and locality preferences.
#[derive(Debug, Clone)]
pub(crate) struct SchedulingState {
    pub(crate) ready: bool,
    pub(crate) degraded: bool,
    pub(crate) credits: usize,
    pub(crate) held: Vec<Uuid7>,
    pub(crate) last_offer: Instant,
}

/// Shared notifications carry no authority; the session retains the database-issued proof.
#[derive(Debug)]
pub(crate) struct SessionHandle {
    pub(crate) runner: Uuid7,
    pub(crate) connection: Uuid7,
    pub(crate) snapshot: watch::Sender<SchedulingState>,
    pub(crate) stop: CancellationToken,
    pub(crate) drain: CancellationToken,
    superseded: CancellationToken,
    offers: Notify,
    rechecks: Notify,
    commands: mpsc::Sender<afd_bus::Command>,
}

impl SessionHandle {
    /// A full command queue becomes a complete durable recheck; it never silently loses authority changes.
    fn command(&self, command: afd_bus::Command) {
        if self.commands.try_send(command).is_err() { self.rechecks.notify_one(); }
    }

    pub(crate) fn request_offer(&self) {
        self.snapshot.send_modify(|state| state.last_offer = Instant::now());
        self.offers.notify_one();
    }

    pub(crate) fn supersede(&self) { self.superseded.cancel(); }
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
    cache: cache::LeaseCache,
    output: StreamWriters,
    accepted: AcceptedKinds,
}

pub(crate) async fn open(gateway: &Gateway, runner: Runner, request: axum::extract::Request)
    -> crate::Result<axum::response::Response>
{
    opening::open(gateway, runner, request).await
}
