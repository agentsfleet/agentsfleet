use std::borrow::Cow;

use afd_wire::runner::{HeartbeatRequest, SelftestCheck, SelftestReport};

use super::requests::capable;

pub(crate) fn view_heartbeat() -> HeartbeatRequest<'static> {
    HeartbeatRequest {
        capability_report: Some(capable()),
        selftest: Some(SelftestReport {
            checks: vec![SelftestCheck {
                name: Cow::Borrowed("network policy is applied"),
                ok: true,
                detail: Cow::Borrowed("the egress boundary answered"),
            }],
            all_ok: true,
            sandbox_tier: Cow::Borrowed("dev_none"),
            network_policy: Cow::Borrowed("allow_all"),
        }),
    }
}
