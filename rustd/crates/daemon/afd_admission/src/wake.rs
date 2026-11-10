//! Notifications follow the commit and cannot revoke acceptance.

use crate::Admissions;

impl Admissions {
    pub(crate) async fn wake(&self, fleet: &af_core::id::Uuid7) {
        afd_bus::wake::publish(&self.bus, fleet.clone()).await;
    }
}
