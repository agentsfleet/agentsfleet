//! The ordered admission pass: may this event run, and on whose money.
//!
//! # The three outcomes, and why they are a type
//!
//! `runBilling` answers `?Billed`. `null` means "the caller answers no-work",
//! and whether the event was TERMINALLY killed is decided by whether the
//! function happened to call `blockEvent(...)` before returning — six of its
//! exits do, four do not. The type carries none of it, so knowing which class
//! an exit belongs to means reading that exit.
//!
//! Here they are [`Admission`], and the durable consequence is applied ONCE, by
//! the caller, as a function of the value. A refusal that forgot to write its
//! row and a refusal that wrote it twice are both unrepresentable.
//!
//! # Order is a money-safety property
//!
//! Every gate that can refuse PERMANENTLY runs before the debit, so a refused
//! event is never charged. The order is one readable sequence in this module,
//! so a reader checks the rule without inferring it from statement order.
//!
//! # What this pass does NOT do
//!
//! It does not resolve the provider. Resolving one reads a tenant's
//! selection, follows it into a vault, and decrypts a key, and none of that is
//! a money DECISION. It arrives
//! here already resolved, on [`Request`], which is what lets every gate below
//! be proven against a database with no vault in the picture.

mod declined;
mod fault;
mod gates;
mod verdict;

pub use self::declined::Declined;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Budget;

use crate::error::Result;
use crate::lease::event::Delivery;
use afd_billing::rates::Posture;
use afd_billing::{Accounts, Charged, Nanos};
use afd_gate::gate::Waiting;

use self::fault::{PAYER, RECEIPT};

/// The event's workspace names no tenant.
const EVENT_TENANT_UNRESOLVED: &str = "lease_tenant_lookup_failed";

/// What the pass decided.
///
/// Four arms because the runner plane genuinely has four answers, and three of
/// them look alike from outside — all end in a no-work reply. They differ in
/// the two ways that matter: whether the event can ever run again, and what an
/// operator should do about it.
///
/// [`Admission::Await`] is durably identical to [`Admission::Retry`] — no row,
/// delivery left leasable — and it is a separate arm anyway, because the two
/// mean opposite things to whoever is watching. A retry says this daemon could
/// not decide and will try again shortly. An await says it decided perfectly
/// well: a human owes an answer, and no amount of retrying will produce one.
/// Collapsing them would put a queue waiting on people into the same graph as a
/// datastore falling over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Issue the lease, billed as described.
    Admit(Billed),
    /// End the event. A human must fix money or configuration before this work
    /// can run, so the caller writes the terminal row and acknowledges the
    /// delivery.
    Refuse(Refusal),
    /// Answer no-work and write nothing. The delivery stays leasable.
    Retry(Transient),
    /// A human has been asked and has not answered.
    ///
    /// Answer no-work, write nothing, and leave the delivery leasable — the
    /// next poll re-reads the recorded gate. No thread waits: the deadline is a
    /// NUMBER carried in the reference, compared against the caller's clock.
    Await(Waiting),
}
/// What the gates resolved, for the lease row and the policy.
///
/// Owned rather than borrowed: it outlives the pass that produced it, and the
/// provider it names is the provider that was BILLED — carrying it forward is
/// what makes "the key we billed is the key we deliver" structural rather than
/// a comment, because there is no second resolution to disagree with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Billed {
    /// The wallet answering for this run.
    pub tenant_id: Uuid7,
    /// Who supplies the provider key.
    pub posture: Posture,
    /// The provider the estimate was priced against.
    pub provider: Box<str>,
    /// The model the estimate was priced against.
    pub model: Box<str>,
    /// What the receive charge drained.
    ///
    /// A VALUE rather than a metric emitted here: the caller records the
    /// credit meter, and an exporter fused into the money path would be one a
    /// run could not be admitted without.
    pub drained: Nanos,
}

/// Why an event was ended, and what an operator is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// The `core.fleet_events.failure_label` value.
    pub label: &'static str,
    /// An operator-readable recovery instruction, or empty.
    ///
    /// Empty renders as SQL `NULL` through the statement's `NULLIF`, which
    /// preserves the established row shape for refusals carrying no
    /// instruction.
    pub detail: &'static str,
}

impl Refusal {
    /// A refusal with no recovery instruction.
    #[must_use]
    pub const fn labelled(label: &'static str) -> Self {
        Self { label, detail: "" }
    }
}

/// Why a pass ended without deciding anything durable.
///
/// Carries the gate name only. The cause is already in the line that gate wrote
/// when it absorbed the fault, and repeating it on the wire would tell a runner
/// about this daemon's datastore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transient {
    /// The gate that could not decide.
    pub at: &'static str,
}

/// What the money pass needs to decide.
///
/// A parameter bundle rather than eight positional arguments: four of them are
/// identifiers or strings of the same shape, which compile clean in any order.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The workspace the work belongs to.
    pub workspace_id: &'a Uuid7,
    /// The fleet whose ceiling applies.
    pub fleet_id: &'a Uuid7,
    /// The event being admitted.
    pub event_id: &'a str,
    /// When the producer raised it.
    pub event_created_at: UnixMillis,
    /// The fleet's declared ceiling, from the config resolved this lease.
    pub budget: Budget,
    /// Who supplies the provider key.
    pub posture: Posture,
    /// The provider resolved for this run.
    pub provider: &'a str,
    /// The model resolved for this run.
    pub model: &'a str,
    /// Whether this delivery is the first.
    pub delivery: Delivery,
}

/// The workspace's payer, as the lease read it: found, unowned, or unread.
///
/// Read once by the lease and handed in, because the provider resolves
/// against the same tenant the gates bill — a second read could only disagree
/// with the first, and it cost a round trip on every lease.
pub type Payer = core::result::Result<Option<Uuid7>, afd_billing::Error>;

/// The payer gate: the read the caller made, judged.
///
/// Found is the tenant to bill. A workspace naming no tenant is a broken
/// foreign key: waiting does not fix it, and running work nobody can be
/// charged for is worse than ending the event. A read that failed retries.
/// The lease calls this before resolving a provider, because a provider is
/// resolved for a tenant and there is none to resolve for otherwise.
///
/// # Errors
/// A read fault the payer gate's posture does not absorb — none today;
/// `.ok_or_else` is what keeps a posture changed to `Admit` from admitting
/// an event it could not find a payer for.
pub fn payer_gate(
    read: Payer,
    workspace_id: &Uuid7,
) -> Result<core::result::Result<Uuid7, Declined>> {
    match read {
        Ok(Some(found)) => Ok(Ok(found)),
        Ok(None) => Ok(Err(unowned_workspace(workspace_id))),
        Err(fault) => PAYER.absorb(&fault).map(Err).ok_or_else(|| fault.into()),
    }
}

/// Run the money gates in the worker's order.
///
/// Payer → balance → fleet budget → receipt. Every gate that can refuse
/// permanently precedes the debit, so a refused event is never charged. The
/// payer gate judges the read the caller made; it reads nothing itself.
///
/// # Errors
/// Datastore faults do NOT reach the caller as `Err` — each gate absorbs its
/// own according to its declared posture. An `Err` here is a fault with no gate
/// to absorb it, which today means an entropy or clock failure while minting
/// the ledger row's identifier.
pub async fn money_gates(
    accounts: &Accounts,
    payer: Payer,
    request: Request<'_>,
    now: UnixMillis,
) -> Result<Admission> {
    let tenant_id = match payer_gate(payer, request.workspace_id)? {
        Ok(found) => found,
        Err(stop) => return Ok(stop.into()),
    };

    if let Some(stop) = gates::balance(accounts, &request, &tenant_id).await {
        return Ok(stop.into());
    }
    if let Some(stop) = gates::fleet_budget(accounts, &request, now).await {
        return Ok(stop.into());
    }

    let drained = match charge(accounts, &request, &tenant_id, now).await {
        Ok(drained) => drained,
        Err(stop) => return Ok(stop.into()),
    };
    Ok(Admission::Admit(Billed {
        tenant_id,
        posture: request.posture,
        provider: request.provider.into(),
        model: request.model.into(),
        drained,
    }))
}

/// Record the receive charge, on a first delivery only.
///
/// Exhaustive over [`Delivery`], which is the whole guard: a redelivery is
/// charged nothing because an earlier delivery already paid, and the balance
/// drain is not replay-guarded the way the ledger row is.
///
/// `Err` carries a [`Declined`] rather than an [`Error`](crate::Error): at
/// this point in the pass a fault has already been absorbed into a decision,
/// and the caller's job is to return it, not to classify it again.
async fn charge(
    accounts: &Accounts,
    request: &Request<'_>,
    tenant_id: &Uuid7,
    now: UnixMillis,
) -> core::result::Result<Nanos, Declined> {
    match request.delivery {
        // A finished event never reaches admission — the pull path acknowledges
        // it and stops before here. Spelled out rather than folded under a `_`
        // so that stays true by compiler rather than by memory: if a caller ever
        // does route one here, it must charge nothing, for the stronger version
        // of `Repeat`'s reason — the run is not merely paid for, it is over.
        Delivery::Repeat | Delivery::Terminal => Ok(Nanos::ZERO),
        Delivery::First => {
            let charged = Charged {
                tenant_id,
                workspace_id: request.workspace_id,
                fleet_id: request.fleet_id,
                event_id: request.event_id,
                posture: request.posture,
                model: request.model,
                event_created_at: request.event_created_at,
            };
            accounts.debit_receive(charged, now).await.map_err(|fault| {
                RECEIPT
                    .absorb(&fault)
                    .unwrap_or(Declined::Retry(Transient { at: RECEIPT.event }))
            })
        }
    }
}

/// The refusal for a workspace that resolves to no tenant.
fn unowned_workspace(workspace_id: &Uuid7) -> Declined {
    let workspace = workspace_id.as_str();
    tracing::warn!(
        event = EVENT_TENANT_UNRESOLVED,
        workspace_id = workspace,
        "the workspace resolves to no tenant; the event cannot be charged to anyone"
    );
    Declined::Refuse(Refusal::labelled(
        afd_core::event::label::TENANT_RESOLVE_FAILED,
    ))
}

#[cfg(test)]
mod tests;
