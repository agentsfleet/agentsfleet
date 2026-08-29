//! The HTTP catalogue seam: the priced model library, read and never written.

use afd_tenant::models::{Boundary, LibraryPage};

/// The model catalogue's one read — a bounded page in normalized order.
///
/// Read-only by construction: the admin CRUD that writes these rows is
/// M179's surface, and a trait with no write verb cannot grow one by
/// accident. Takes ALREADY-PARSED values — the filter is normalized and the
/// boundary is a decoded cursor — so there is no validation arm in any
/// implementation, and none a stub could get differently right.
pub trait ModelCatalogue: Send + Sync + std::fmt::Debug + 'static {
    /// One bounded page of the catalogue.
    ///
    /// # Errors
    /// Reports a datastore that would not answer — under the library
    /// family's own code — and a row this daemon cannot read.
    fn page(
        &self,
        filter: Option<&str>,
        after: Option<&Boundary>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<LibraryPage>> + Send;
}

/// The production surface answers it directly.
impl ModelCatalogue for afd_tenant::models::Models {
    fn page(
        &self,
        filter: Option<&str>,
        after: Option<&Boundary>,
        limit: u32,
    ) -> impl Future<Output = afd_tenant::Result<LibraryPage>> + Send {
        Self::page(self, filter, after, limit)
    }
}
