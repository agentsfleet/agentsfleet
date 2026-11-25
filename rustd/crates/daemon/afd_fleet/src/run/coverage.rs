//! Whether a run in flight may buy another slice: the payer's balance.
//!
//! The same gate the run verb applies at issue, asked again mid-run — and
//! asked again on purpose. A run admitted an hour ago may have drained the
//! wallet it was admitted against, so the balance is read LIVE rather than
//! pinned on the run. The payer's balance is the only ceiling a run has.
//!
//! # A balance that cannot be read admits
//!
//! A metering outage that killed every run in flight would turn a billing
//! incident into a platform incident, and a run given one more thirty-second
//! slice is recoverable where a terminated agent is not. A payer with no
//! wallet row is refused: it can fund nothing.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use afd_billing::biller::{Account, Ask, Funds, WorkKey};
use afd_billing::rates::Estimate;
use jiff::Timestamp;

use crate::error::{Result, renewal_no_credits};
use crate::run::pull::RunService;
use crate::run::renew::Renewing;
use crate::run::usage::Pricing;

/// The tenant's pool cannot fund another slice.
const EVENT_NO_CREDITS: &str = "renew_no_credits";

/// A gate could not be read, and the renewal was admitted anyway.
const EVENT_GATE_UNREADABLE: &str = "renew_gate_unreadable_admitted";

impl RunService {
    /// The money gate a renewal must pass.
    ///
    /// # Errors
    /// Refuses with [`renewal_no_credits`](crate::error) when the payer's
    /// balance is spent or the payer has no wallet row.
    pub(super) async fn gate_renewal(
        &self,
        run: &Renewing,
        run_key: &Uuid7,
        run_id: &str,
        now: Timestamp,
    ) -> Result<()> {
        self.funds_cover(run, run_key, run_id, now).await
    }

    /// Whether the payer can fund one more slice.
    ///
    /// The balance, every line the payer has logged already debited, must cover the same floor the issue gate admits against,
    /// so issue and renewal share one credit policy. Nothing is set aside: a
    /// payer whose parallel runs spent past the balance since their last
    /// renewals carries that as a debt, and this check stops each run at its
    /// next renewal.
    ///
    /// A tenant with no wallet row is refused, as the issue gate refuses one.
    /// An outage the biller's outage policy admitted answers
    /// [`Funds::Unconfirmed`] and renews.
    pub(crate) async fn funds_cover(
        &self,
        run: &Renewing,
        run_key: &Uuid7,
        run_id: &str,
        now: Timestamp,
    ) -> Result<()> {
        let posture = Pricing::posture_of(&run.posture)?;
        let estimate = self
            .accounts
            .estimate(posture, &run.provider, &run.model, now)
            .await
            .map_err(|fault| std::sync::Arc::new(crate::Error::from(fault)));
        self.funds_cover_at(run, run_key, run_id, estimate).await
    }

    /// [`Self::funds_cover`] against a floor the caller already priced, as the
    /// host batch does from its cached book. A floor that could not
    /// be priced fails open, as an unreadable balance does.
    pub(crate) async fn funds_cover_at(
        &self,
        run: &Renewing,
        run_key: &Uuid7,
        run_id: &str,
        estimate: core::result::Result<Estimate, std::sync::Arc<crate::Error>>,
    ) -> Result<()> {
        let estimate = match estimate {
            Ok(estimate) => estimate,
            Err(fault) => {
                admit_unreadable("balance", run, run_id, &fault);
                return Ok(());
            }
        };
        let ask = Ask {
            work: WorkKey::FleetRun(run_key.clone()),
            payer: Account {
                tenant: run.tenant_id.clone(),
                workspace: run.workspace_id.clone(),
            },
            next: estimate.floor(),
        };
        let funds = self.biller.check(&ask).await;

        match funds {
            Ok(Funds::Covered | Funds::Unconfirmed) => Ok(()),
            Err(fault) if fault.is_wallet_missing() => Err(short(run, run_id)),
            Ok(Funds::Short { .. }) => Err(short(run, run_id)),
            // Fail OPEN: the balance could not be read at all, which says
            // nothing about whether it covers the run.
            Err(fault) => {
                admit_unreadable("balance", run, run_id, &crate::Error::from(fault));
                Ok(())
            }
        }
    }
}

/// The refusal a payer who cannot fund another slice earns, said aloud.
fn short(run: &Renewing, run_id: &str) -> crate::Error {
    let fleet = run.fleet_id.as_str();
    {
        let (fleet_id, event, message) = (
            &fleet,
            &EVENT_NO_CREDITS,
            format_args!("the tenant's balance can no longer fund this run; not renewed"),
        );
        tracing::warn!(message, fleet_id, run_id, event);
    };
    renewal_no_credits()
}

/// Admit a renewal whose gate could not be read, and say so.
///
/// One place, so the fail-open posture is applied identically at every call
/// site and an operator greps one event for every instance of it. Logs and
/// nothing else — each call site states its own `Ok(())` beside it, so the
/// admission is visible AT the arm that takes it rather than inferred from a
/// return type three screens further down.
fn admit_unreadable(gate: &'static str, run: &Renewing, run_id: &str, fault: &crate::Error) {
    let fleet = run.fleet_id.as_str();
    let Logged { error_code, reason } = fault.logged();
    {
        let (fleet_id, event, message) = (
            &fleet,
            &EVENT_GATE_UNREADABLE,
            format_args!(
                "a renewal gate could not be read; the run is admitted for one more slice"
            ),
        );
        tracing::warn!(message, error_code, fleet_id, run_id, gate, reason, event);
    };
}

#[cfg(test)]
mod tests {
    use af_core::id::{ENTROPY_LEN, Uuid7};
    use jiff::Timestamp;

    use super::admit_unreadable;
    use crate::run::renew::Renewing;

    /// A run for the reporting helper, with three distinct identifiers.
    ///
    /// Distinct because the warning names the fleet and the run separately,
    /// and a fixture that reused one id would let a swapped field pass.
    fn renewing() -> Result<Renewing, &'static str> {
        let at = Timestamp::constant(1_767_225_600, 0);
        let mint = |seed: u8| {
            Uuid7::encode(at, [seed; ENTROPY_LEN])
                .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")
        };
        Ok(Renewing {
            cancelled: false,
            tenant_id: mint(1)?,
            fleet_id: mint(2)?,
            workspace_id: mint(3)?,
            posture: "sandboxed".to_owned(),
            provider: "anthropic".to_owned(),
            model: "claude-opus-5".to_owned(),
            status: "active".to_owned(),
        })
    }

    /// The unreadable-gate admission renders every refusal it can be handed.
    ///
    /// The call sites pass a balance failure, which is admitted for one more
    /// slice rather than refused — so what there is to prove is that the
    /// reporting itself survives every error kind. Both
    /// `fault.to_string()` and `fault.code()` run arbitrary per-kind code at the
    /// moment a gate has already failed, which is the worst moment for either
    /// to panic.
    #[test]
    fn the_unreadable_gate_admission_renders_every_refusal() -> Result<(), &'static str> {
        let run = renewing()?;

        for fault in [
            crate::error::stale_fence(),
            crate::error::run_not_found(),
            crate::error::run_lost(),
            crate::error::renewal_no_credits(),
        ] {
            assert!(
                !fault.to_string().is_empty(),
                "the refusal renders to something a reader can act on"
            );
            let code = fault.code().as_str();
            assert!(!code.is_empty(), "{code:?}");
            admit_unreadable("balance", &run, "run-fixture", &fault);
        }
        Ok(())
    }

    #[test]
    fn an_unreadable_renewal_gate_reason_carries_no_code_prefix() -> Result<(), &'static str> {
        let log = af_core::test_util::trace::Capture::install();
        let run = renewing()?;
        let fault = crate::error::query("balance read")(sqlx::Error::PoolTimedOut);

        admit_unreadable("balance", &run, "run-fixture", &fault);

        let line = log.only(super::EVENT_GATE_UNREADABLE);
        assert_eq!(line.field("error_code"), Some(fault.code().as_str()));
        assert_eq!(line.field("reason"), Some(fault.told().as_str()));
        Ok(())
    }

    // Value: protects=a balance the ledger could not read admits one more slice
    // and says which gate failed open; fails_when=the outage arm refuses the run
    // or admits it silently; why_new=no test failed the balance read itself;
    // seam=`funds_cover_at` over a ledger nobody answers on.
    #[cfg(feature = "test-util")]
    #[tokio::test]
    async fn an_unreadable_balance_admits_the_slice_and_says_so() -> Result<(), &'static str> {
        let log = af_core::test_util::trace::Capture::install();
        let run = renewing()?;
        let floor = afd_billing::rates::Estimate::Priced(afd_billing::Nanos::from_i64(1));

        let admitted = crate::run::test_dead::plane()
            .funds_cover_at(&run, &run.fleet_id, "run-fixture", Ok(floor))
            .await;

        assert!(admitted.is_ok(), "{:?}", admitted.err());
        let line = log.only(super::EVENT_GATE_UNREADABLE);
        assert_eq!(line.field("gate"), Some("balance"));
        assert_eq!(line.field("run_id"), Some("run-fixture"));
        assert_eq!(
            line.field("error_code"),
            Some(af_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
        Ok(())
    }
}
