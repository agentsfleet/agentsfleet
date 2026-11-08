//! The poll's status spelling against the store's own.

use afd_tenant::session::SessionStatus;

use super::status_name;

/// Every state the store names, so a new one cannot pass unspelled.
const EVERY_STATUS: [SessionStatus; 5] = [
    SessionStatus::Pending,
    SessionStatus::VerificationPending,
    SessionStatus::Consumed,
    SessionStatus::Expired,
    SessionStatus::Aborted,
];

/// The wire spelling is the one the store serializes, for every state: two
/// vocabularies that drifted would put a status on a 200 that the CLI, which
/// reads the store's spelling elsewhere, does not recognise.
#[test]
fn every_status_is_spelled_as_the_store_spells_it() {
    for status in EVERY_STATUS {
        assert_eq!(
            serde_json::to_value(status).ok(),
            Some(serde_json::Value::from(status_name(status))),
            "{status:?}"
        );
    }
}
