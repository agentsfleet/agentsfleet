//! The HTTP billing seam: the tenant's money, read and never moved.

use afd_billing::tenant::cursor::Boundary;
use afd_billing::tenant::{ChargeRow, Wallet};
use afd_core::id::Uuid7;

/// The tenant's billing reads — a wallet snapshot and one page of charges.
///
/// Read-only by construction: neither method can move money, and
/// `afd_billing::tenant` documents why the write half lives on the lease
/// plane instead. Both take ALREADY-PARSED values — the limit is bounded and
/// the boundary is a decoded cursor — so there is no validation arm in any
/// implementation, and none a stub could get differently right from the real
/// one.
pub trait TenantBilling: Send + Sync + std::fmt::Debug + 'static {
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
