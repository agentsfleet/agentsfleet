//! A setup turn's failure: which reason it stores, which faults fail it rather
//! than roll its assignment back, and what the failure logs.

use af_core::{error_code, id::Uuid7, test_util::trace::Capture};
use af_wire::{report::FailureClass, setup::FailureReason};

use super::{EVENT_TURN_FAILED, SetupFinish, log_turn_failed, reason_word};
use crate::error::{ErrorKind, raise};

/// The draft a failure record names.
const DRAFT: &str = "0199a0b0-0000-7000-8000-0000000000d1";

/// A run failed for its frame keeps that reason, so the person is told to
/// shorten the message; every other class, and none, is the provider's.
#[test]
fn a_frame_too_large_report_keeps_its_reason_and_any_other_is_the_providers() {
    for (class, reason) in [
        (
            Some(FailureClass::FrameTooLarge),
            FailureReason::FrameTooLarge,
        ),
        (
            Some(FailureClass::RunnerCrash),
            FailureReason::ProviderFailed,
        ),
        (
            Some(FailureClass::TimeoutKill),
            FailureReason::ProviderFailed,
        ),
        (None, FailureReason::ProviderFailed),
    ] {
        assert_eq!(SetupFinish::failure_of(class), reason, "{class:?}");
    }
    assert_eq!(reason_word(FailureReason::FrameTooLarge), "frame_too_large");
}

/// Turns store these words, and readers parse them back through serde:
/// every arm must spell its wire form.
#[test]
fn every_stored_failure_word_is_its_wire_spelling() {
    for value in [
        FailureReason::InsufficientBalance,
        FailureReason::ModelUnavailable,
        FailureReason::ProviderFailed,
        FailureReason::ProviderUsageUnknown,
        FailureReason::FrameTooLarge,
    ] {
        let wire = serde_json::to_value(value).ok();
        assert_eq!(
            wire.as_ref().and_then(serde_json::Value::as_str),
            Some(reason_word(value))
        );
    }
}

/// A key that will not open, as the credential plane reports it.
fn unopened_key() -> Result<crate::Error, &'static str> {
    let crypto = af_crypto::secret::Kek::from_hex("zz")
        .err()
        .ok_or("a two-character non-hex string unexpectedly parsed as a KEK")?;
    Ok(crate::Error::from(afd_credential::Error::from(
        afd_vault::Error::from(crypto),
    )))
}

/// The turn's own faults fail it, each under its reason; a statement that
/// failed is the datastore's, and rolls the assignment back instead.
#[test]
fn a_turns_own_fault_fails_it_and_a_datastore_fault_does_not() -> Result<(), &'static str> {
    let stored = serde_json::from_str::<u8>("{")
        .map_err(raise::stored)
        .err()
        .ok_or("a brace parsed as a number")?;
    for (label, error, reason) in [
        (
            "model",
            crate::Error::from(ErrorKind::Model),
            Some(FailureReason::ModelUnavailable),
        ),
        (
            "balance",
            crate::Error::from(ErrorKind::Balance),
            Some(FailureReason::InsufficientBalance),
        ),
        (
            "key",
            unopened_key()?,
            Some(FailureReason::ModelUnavailable),
        ),
        ("candidate", stored, Some(FailureReason::ProviderFailed)),
        (
            "statement",
            raise::query("assign a setup turn")(sqlx::Error::PoolClosed),
            None,
        ),
        ("hidden", crate::Error::from(ErrorKind::Hidden), None),
    ] {
        assert_eq!(error.turn_failure(), reason, "{label}");
    }
    Ok(())
}

/// A failure that is its reason's own logs that reason's code; one caused by
/// another failure logs its own code with the cause's beside it; and one with
/// no code of its own logs the cause's alone.
#[test]
fn a_failed_turn_logs_its_own_code_and_its_cause() -> Result<(), Box<dyn std::error::Error>> {
    let draft = Uuid7::parse(DRAFT)?;
    let captured = Capture::install();
    log_turn_failed(
        &draft,
        FailureReason::ModelUnavailable,
        error_code::INTERNAL_OPERATION_FAILED,
    );
    log_turn_failed(
        &draft,
        FailureReason::InsufficientBalance,
        error_code::SETUP_BALANCE,
    );
    log_turn_failed(
        &draft,
        FailureReason::ProviderFailed,
        error_code::INTERNAL_DB_QUERY,
    );
    let logged: Vec<_> = captured
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_TURN_FAILED))
        .collect();
    drop(captured);

    let fields: Vec<_> = logged
        .iter()
        .map(|event| {
            (
                event.field("error_code"),
                event.field("cause_error_code"),
                event.field("reason"),
                event.field("draft_id"),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            (
                Some("UZ-SETUP-008"),
                Some("UZ-INTERNAL-003"),
                Some("model_unavailable"),
                Some(DRAFT)
            ),
            (
                Some("UZ-SETUP-007"),
                None,
                Some("insufficient_balance"),
                Some(DRAFT)
            ),
            (
                Some("UZ-INTERNAL-002"),
                None,
                Some("provider_failed"),
                Some(DRAFT)
            ),
        ]
    );
    assert!(
        logged
            .iter()
            .all(|event| event.level == tracing::Level::WARN)
    );
    Ok(())
}
