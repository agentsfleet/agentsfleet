//! The ordered admission pass: may this event run, and on whose money.
//!
//! # The four outcomes, and why they are a type
//!
//! `runBilling` answers `?Billed`. `null` means "the caller answers no-work",
//! and whether the event was TERMINALLY killed is decided by whether the
//! function happened to call `blockEvent(...)` before returning — six of its
//! exits do, four do not. The type carries none of it, so knowing which class
//! an exit belongs to means reading that exit.
//!
//! Here the four are [`Admission`], and the durable consequence is applied
//! ONCE, by the caller, as a function of the value. A refusal that forgot to
//! write its row and a refusal that wrote it twice are both unrepresentable.
//!
//! # Fault posture is declared, not caught
//!
//! `balanceCoversEstimate` contains three separate `catch return true` — on the
//! acquire, on the wallet load, and on the rate lookup. Each is a fail-open
//! decision a reviewer has to notice individually, and a fourth statement added
//! later inherits nothing.
//!
//! Here each gate carries a [`Gate`] value naming what its own failure means,
//! and [`Gate::absorb`] applies it. The posture is a fact next to the gate's
//! name rather than a property of ten `catch` sites.
//!
//! # Order is a money-safety property
//!
//! Every gate that can refuse PERMANENTLY runs before the debit, so a refused
//! event is never charged. `budget.zig` states the same rule about its own
//! position — "before the receive debit, so a refused event is never charged" —
//! and here the order is one readable sequence rather than something inferred
//! from statement order across a hundred and twenty lines.

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_fleet_runtime::config::Budget;

use crate::error::{Error, Result};
use crate::lease::event::Delivery;
use crate::money::budget::{self, Verdict};
use crate::money::rates::Posture;
use crate::money::{Accounts, Charged, Nanos};
use crate::sql;

/// The credit pool would not answer.
///
/// `LOGGING_STANDARD.md` §3 `event` values, one declaration each (RULE UFS),
/// spelled as `service_billing.zig` spells them so a dashboard built against
/// the Zig daemon keeps matching after the cutover.
const EVENT_BALANCE_UNAVAILABLE: &str = "lease_balance_unavailable";

/// The fleet's drain would not answer.
const EVENT_BUDGET_UNAVAILABLE: &str = "lease_budget_unavailable";

/// The credit pool cannot cover this run.
const EVENT_BALANCE_EXHAUSTED: &str = "lease_balance_exhausted";

/// The fleet has reached its own ceiling.
const EVENT_BUDGET_BREACH: &str = "lease_budget_breach";

/// The event's workspace names no tenant.
const EVENT_TENANT_UNRESOLVED: &str = "lease_tenant_lookup_failed";

/// What the pass decided.
///
/// Four arms because the runner plane genuinely has four answers, and the two
/// that look alike from the outside — both end in a no-work reply — differ in
/// the only way that matters: whether the event can ever run again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// Issue the lease, billed as described.
    Admit(Billed),
    /// End the event. A human must fix money or configuration before this work
    /// can run, so the caller writes the terminal row and acknowledges the
    /// delivery.
    Refuse(Refusal),
    /// Answer no-work and write nothing. The delivery stays leasable and the
    /// next poll retries.
    Retry(Transient),
}

/// What the gates resolved, for the lease row and the policy.
///
/// Owned rather than borrowed: it outlives the pass that produced it, and the
/// provider it names is the provider that was BILLED — carrying it forward is
/// what makes "the key we billed is the key we deliver" structural rather than
/// a comment. There is no second resolution to disagree with.
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
    /// Returned as a VALUE rather than emitted as a metric here. The credit
    /// meter belongs to §6/M181, and fusing the two is what makes
    /// `service_billing.zig` unable to run its money path without an exporter
    /// configured.
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
    /// preserves the established row shape for the refusals that carry no
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
/// Carries the event name only. The cause is already in the log line the gate
/// wrote when it absorbed the fault, and repeating it in the wire reply would
/// tell a runner about this daemon's datastore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transient {
    /// The gate that could not decide.
    pub at: &'static str,
}

/// What a gate's own failure decides.
///
/// Two arms and not three: a DATASTORE fault never ends an event permanently.
/// Permanence comes from verdicts — an exhausted wallet, a breached ceiling, a
/// workspace with no tenant — which arrive as `Ok`, not as `Err`. That split is
/// RULE ECL held at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnFault {
    /// Admit the event. The two MONEY gates take this posture: a metering
    /// outage must not halt every fleet on the platform, and a gate stricter
    /// than the platform's own credit pool would be an inconsistent guarantee.
    Admit,
    /// Answer no-work. The delivery stays leasable.
    Retry,
}

/// One gate, and what its failure means.
///
/// A value rather than a convention, so the posture is legible beside the
/// gate's name and a new gate has to state one.
#[derive(Debug, Clone, Copy)]
struct Gate {
    /// The `event` a fault at this gate logs under.
    event: &'static str,
    /// What a fault here decides.
    on_fault: OnFault,
}

/// The tenant's credit pool. Fails OPEN.
const BALANCE: Gate = Gate {
    event: EVENT_BALANCE_UNAVAILABLE,
    on_fault: OnFault::Admit,
};

/// The fleet's own declared ceiling. Fails OPEN, mirroring [`BALANCE`] — a
/// budget gate stricter than the credit gate above it would be an inconsistent
/// guarantee.
const BUDGET: Gate = Gate {
    event: EVENT_BUDGET_UNAVAILABLE,
    on_fault: OnFault::Admit,
};

/// Recording the receive charge. Fails CLOSED-to-retry: an uncharged run must
/// not proceed, but nothing is terminal — the next poll charges it.
const RECEIPT: Gate = Gate {
    event: "lease_receive_debit_unavailable",
    on_fault: OnFault::Retry,
};

/// Resolving who pays. Fails CLOSED-to-retry.
const PAYER: Gate = Gate {
    event: EVENT_TENANT_UNRESOLVED,
    on_fault: OnFault::Retry,
};

impl Gate {
    /// Apply this gate's declared posture to a fault, logging it once.
    ///
    /// `None` means the pass continues — which for [`OnFault::Admit`] IS the
    /// fail-open decision, taken here rather than by a `catch` returning a
    /// bare `true` eight frames down.
    fn absorb(self, fault: &Error) -> Option<Admission> {
        // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
        // scores the dead copy.
        let code = error_code::INTERNAL_DB_QUERY.as_str();
        let reason = fault.to_string();
        let event = self.event;
        match self.on_fault {
            OnFault::Admit => {
                tracing::warn!(
                    error_code = code,
                    event,
                    reason,
                    "a money gate could not be read; the event is admitted rather than \
                     halting every fleet on the platform"
                );
                None
            }
            OnFault::Retry => {
                tracing::warn!(
                    error_code = code,
                    event,
                    reason,
                    "the pass ended early; the delivery stays leasable and the next poll retries"
                );
                Some(Admission::Retry(Transient { at: event }))
            }
        }
    }
}

/// What the money pass needs to decide.
///
/// One struct rather than eight positional parameters: four of them are
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

/// Run the money gates in the worker's order.
///
/// Tenant → balance → budget → receipt. Every gate that can refuse permanently
/// precedes the debit, so a refused event is never charged.
///
/// # Errors
/// None of the datastore faults reach the caller as `Err` — each gate absorbs
/// its own according to its declared posture. An `Err` here is a fault with no
/// gate to absorb it, which today means only an entropy or clock failure while
/// minting the ledger row's identifier.
pub async fn money_gates(
    accounts: &Accounts,
    request: Request<'_>,
    now: UnixMillis,
) -> Result<Admission> {
    let tenant_id = match accounts.payer(request.workspace_id).await {
        // A workspace naming no tenant is a broken foreign key: waiting does
        // not fix it, and running work nobody can be charged for is worse than
        // ending the event.
        Ok(None) => {
            let workspace = request.workspace_id.as_str();
            tracing::warn!(
                event = EVENT_TENANT_UNRESOLVED,
                workspace_id = workspace,
                "the workspace resolves to no tenant; the event cannot be charged to anyone"
            );
            return Ok(Admission::Refuse(Refusal::labelled(
                sql::event::label::TENANT_RESOLVE_FAILED,
            )));
        }
        Ok(Some(found)) => found,
        Err(fault) => match PAYER.absorb(&fault) {
            Some(stop) => return Ok(stop),
            // `PAYER` is a retry gate, so `absorb` always stops. Propagating
            // keeps the arm total rather than asserting the posture twice.
            None => return Err(fault),
        },
    };

    if let Some(stop) = balance_gate(accounts, &request, &tenant_id).await {
        return Ok(stop);
    }
    if let Some(stop) = budget_gate(accounts, &request, now).await {
        return Ok(stop);
    }

    let charged = Charged {
        tenant_id: &tenant_id,
        workspace_id: request.workspace_id,
        fleet_id: request.fleet_id,
        event_id: request.event_id,
        posture: request.posture,
        model: request.model,
        event_created_at: request.event_created_at,
    };
    // Exhaustive over `Delivery`, which is the whole guard: a redelivery is
    // charged nothing because an earlier delivery already paid, and the balance
    // drain is not replay-guarded the way the ledger row is.
    let drained = match request.delivery {
        Delivery::First => match accounts.debit_receive(charged, now).await {
            Ok(drained) => drained,
            Err(fault) => match RECEIPT.absorb(&fault) {
                Some(stop) => return Ok(stop),
                None => return Err(fault),
            },
        },
        Delivery::Repeat => Nanos::ZERO,
    };

    Ok(Admission::Admit(Billed {
        tenant_id,
        posture: request.posture,
        provider: request.provider.into(),
        model: request.model.into(),
        drained,
    }))
}

/// The tenant's credit pool against this run's floor cost.
///
/// `None` continues the pass. A tenant with no wallet row is ADMITTED — see
/// [`crate::money::wallet`] — and so is one whose model the catalogue cannot
/// price, because an estimate is not a charge.
async fn balance_gate(
    accounts: &Accounts,
    request: &Request<'_>,
    tenant_id: &Uuid7,
) -> Option<Admission> {
    let estimate = match accounts
        .estimate(request.posture, request.provider, request.model)
        .await
    {
        Ok(estimate) => estimate,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    // An unpriceable model costs zero to admit against, so the wallet read is
    // still worth taking: it is what makes an exhausted pool refuse even when
    // the catalogue has moved.
    let wallet = match accounts.wallet(tenant_id).await {
        Ok(wallet) => wallet,
        Err(fault) => return BALANCE.absorb(&fault),
    };
    let covers = wallet.is_none_or(|held| held.balance.covers(estimate.floor()));
    if covers {
        return None;
    }
    let fleet = request.fleet_id.as_str();
    let event = request.event_id;
    let floor = estimate.floor().as_i64();
    tracing::debug!(
        event = EVENT_BALANCE_EXHAUSTED,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        estimate_nanos = floor,
        "the tenant's credit pool cannot cover this run's floor cost"
    );
    Some(Admission::Refuse(Refusal::labelled(
        sql::event::label::BALANCE_EXHAUSTED,
    )))
}

/// The fleet's own ceiling against what it has already drained.
///
/// Checked AFTER the tenant's credit pool and BEFORE the receive debit: a
/// refused event must never be charged, and the two gates are independent —
/// both must pass.
async fn budget_gate(
    accounts: &Accounts,
    request: &Request<'_>,
    now: UnixMillis,
) -> Option<Admission> {
    let spend = match accounts
        .spend(request.workspace_id, request.fleet_id, now)
        .await
    {
        Ok(spend) => spend,
        Err(fault) => return BUDGET.absorb(&fault),
    };
    let verdict = budget::covers(request.budget, spend);
    if verdict == Verdict::Admit {
        return None;
    }
    let code = error_code::RUN_BUDGET_EXCEEDED.as_str();
    let fleet = request.fleet_id.as_str();
    let event = request.event_id;
    let which = verdict.as_str();
    tracing::debug!(
        error_code = code,
        event = EVENT_BUDGET_BREACH,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        verdict = which,
        "the fleet has reached a ceiling its author declared"
    );
    Some(Admission::Refuse(Refusal::labelled(
        sql::event::label::BUDGET_BREACH,
    )))
}
