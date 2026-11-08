//! Notifications follow the commit and cannot revoke acceptance.

use crate::Admissions;

impl Admissions {
    pub(crate) async fn wake(&self, fleet: &str) {
        afd_dragonfly::doorbell::publish(&self.queue, fleet).await;
    }
}
