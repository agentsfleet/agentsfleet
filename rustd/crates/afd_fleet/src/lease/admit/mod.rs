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
//! Every gate that can refuse PERMANENTLY runs before the lease is issued and
//! its hold placed, so a refused event is never charged. The order is one
//! readable sequence in this module, so a reader checks the rule without
//! inferring it from statement order.
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

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Budget;
use jiff::Timestamp;

use crate::error::Result;
use afd_billing::biller::Biller;
use afd_billing::rates::Posture;
use afd_billing::{Accounts, Nanos};
use afd_gate::gate::Waiting;

use self::fault::PAYER;

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
    /// The floor the balance gate admitted against: what the lease's first
    /// hold reserves once the lease is issued.
    pub floor: Nanos,
}

/// Why an event was ended, and what an operator is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// The `fleet.events.failure_label` value.
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
    /// The fleet's declared ceiling, from the config resolved this lease.
    pub budget: Budget,
    /// Who supplies the provider key.
    pub posture: Posture,
    /// The provider resolved for this run.
    pub provider: &'a str,
    /// The model resolved for this run.
    pub model: &'a str,
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
/// Payer → balance → fleet budget. Every gate that can refuse permanently
/// runs before the lease is issued, so a refused event is never charged. The
/// payer gate judges the read the caller made; it reads nothing itself. Nothing
/// is charged here: the lease's hold is placed once it is issued, and its usage
/// is charged as it runs.
///
/// # Errors
/// Datastore faults do NOT reach the caller as `Err` — each gate absorbs its
/// own according to its declared posture. An `Err` here is a fault with no gate
/// to absorb it, which today is a payer read the payer gate does not absorb.
pub async fn money_gates(
    accounts: &Accounts,
    biller: &dyn Biller,
    payer: Payer,
    request: Request<'_>,
    now: Timestamp,
) -> Result<Admission> {
    let tenant_id = match payer_gate(payer, request.workspace_id)? {
        Ok(found) => found,
        Err(stop) => return Ok(stop.into()),
    };

    let floor = match gates::balance(accounts, biller, &request, &tenant_id, now).await {
        Ok(floor) => floor,
        Err(stop) => return Ok(stop.into()),
    };
    if let Some(stop) = gates::fleet_budget(accounts, &request).await {
        return Ok(stop.into());
    }

    Ok(Admission::Admit(Billed {
        tenant_id,
        posture: request.posture,
        provider: request.provider.into(),
        model: request.model.into(),
        floor,
    }))
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
