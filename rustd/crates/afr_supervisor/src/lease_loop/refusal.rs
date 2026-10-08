//! How a lease that cannot go on ends: the failure logged under its registry
//! code, and the ending its report carries.

use afd_core::error_code::Coded;
use afd_wire::report::FailureClass;
use afr_agent::Unhosted;

use super::{
    DETAIL_BLOCKED_ENDPOINT, DETAIL_UNHOSTED, DETAIL_UNHOSTED_PROVIDER, EVENT_UNHOSTED,
    EVENT_UNHOSTED_PROVIDER, LeaseRun,
};
use crate::report::Ending;

impl LeaseRun<'_> {
    /// Logs a lease whose policy names a tool or a model provider the
    /// engine cannot host, and ends it before anything was prepared for it.
    pub(super) fn unhosted(&self, refusal: &afr_agent::Error) -> Ending {
        let (event, detail, name) = match refusal.unhosted() {
            Some(Unhosted::Provider(name)) => (
                EVENT_UNHOSTED_PROVIDER,
                DETAIL_UNHOSTED_PROVIDER,
                Some(name),
            ),
            Some(Unhosted::Endpoint(name)) => {
                (EVENT_UNHOSTED_PROVIDER, DETAIL_BLOCKED_ENDPOINT, Some(name))
            }
            Some(Unhosted::Tool(name)) => (EVENT_UNHOSTED, DETAIL_UNHOSTED, Some(name)),
            None => (EVENT_UNHOSTED, DETAIL_UNHOSTED, None),
        };
        let code = refusal.code().as_str();
        let lease_id = self.ids.lease.as_str();
        tracing::error!(error_code = code, lease_id, name, event);
        failed(FailureClass::StartupPosture, detail)
    }

    /// Logs why a lease could not start, and ends it at startup.
    pub(super) fn refuse(
        &self,
        failure: &impl Coded,
        event: &'static str,
        detail: &'static str,
    ) -> Ending {
        self.fail(failure, FailureClass::StartupPosture, event, detail)
    }

    /// Logs a failure from any crate the lease runs through, and ends the
    /// lease as `class`.
    pub(super) fn fail(
        &self,
        failure: &impl Coded,
        class: FailureClass,
        event: &'static str,
        detail: &'static str,
    ) -> Ending {
        let code = failure.code().as_str();
        let lease_id = self.ids.lease.as_str();
        let reason = failure.told();
        tracing::warn!(error_code = code, lease_id, reason, event, detail);
        failed(class, detail)
    }
}

/// An ending that never ran the turn to its end.
pub(super) const fn failed(class: FailureClass, detail: &'static str) -> Ending {
    Ending::Failed { class, detail }
}
