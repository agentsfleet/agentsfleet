//! The HTTP billing seam: the tenant's money, read and never moved.

use afd_billing::Wallet;
use afd_billing::tenant::ChargeRow;
use afd_billing::tenant::cursor::Boundary;
use afd_core::id::Uuid7;

/// The tenant's wallet, raw charges and settled usage reports.
///
/// Read-only by construction: no method can move money, and
/// `afd_billing::tenant` documents why the write half lives on the lease
/// plane instead. Each takes already-parsed values — the limit is bounded and
/// the boundary is a decoded cursor — so there is no validation arm in any
/// implementation, and none a stub could get differently right from the real
/// one.
pub trait TenantBilling: Send + Sync + std::fmt::Debug + 'static {
    /// Full-period settled totals and a bounded activity page.
    ///
    /// # Errors
    /// Reports an unavailable datastore or an unreadable ledger row.
    fn report(
        &self,
        tenant: &Uuid7,
        request: &afd_billing::report::Request,
    ) -> impl Future<Output = afd_billing::Result<afd_billing::report::Report>> + Send;

    /// The individually settled lines of one activity.
    ///
    /// # Errors
    /// Reports an unavailable datastore or an unreadable ledger row.
    fn activity_charges(
        &self,
        tenant: &Uuid7,
        work: &afd_billing::biller::WorkKey,
        request: &afd_billing::report::Request,
        after: Option<i64>,
    ) -> impl Future<Output = afd_billing::Result<Vec<afd_billing::report::Charge>>> + Send;

    /// The wallet snapshot behind `GET /v1/tenants/me/billing`.
    ///
    /// # Errors
    /// Refuses a tenant with no wallet row as the bootstrap-invariant
    /// violation it is. Reports a datastore that would not answer.
    fn snapshot(&self, tenant: &Uuid7) -> impl Future<Output = afd_billing::Result<Wallet>> + Send;

    /// One page of the tenant's charges, newest first.
    ///
    /// `boundary` is the decoded cursor, when the caller is resuming — the
    /// handler parses the token so a malformed one is refused before a
    /// connection is drawn.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon
    /// cannot read.
    fn charges(
        &self,
        tenant: &Uuid7,
        limit: u32,
        boundary: Option<&Boundary>,
    ) -> impl Future<Output = afd_billing::Result<Vec<ChargeRow>>> + Send;
}

/// The production surface answers it directly.
impl TenantBilling for afd_billing::tenant::Billing {
    fn report(
        &self,
        tenant: &Uuid7,
        request: &afd_billing::report::Request,
    ) -> impl Future<Output = afd_billing::Result<afd_billing::report::Report>> + Send {
        Self::report(self, tenant, request)
    }

    fn activity_charges(
        &self,
        tenant: &Uuid7,
        work: &afd_billing::biller::WorkKey,
        request: &afd_billing::report::Request,
        after: Option<i64>,
    ) -> impl Future<Output = afd_billing::Result<Vec<afd_billing::report::Charge>>> + Send {
        Self::activity_charges(self, tenant, work, request, after)
    }

    fn snapshot(&self, tenant: &Uuid7) -> impl Future<Output = afd_billing::Result<Wallet>> + Send {
        Self::snapshot(self, tenant)
    }

    fn charges(
        &self,
        tenant: &Uuid7,
        limit: u32,
        boundary: Option<&Boundary>,
    ) -> impl Future<Output = afd_billing::Result<Vec<ChargeRow>>> + Send {
        Self::charges(self, tenant, limit, boundary)
    }
}
