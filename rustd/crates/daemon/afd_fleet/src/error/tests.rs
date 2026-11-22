//! Behavioural coverage for the runner-plane error taxonomy.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use std::error::Error as _;

use af_core::error_code::{self, ErrorCode};

use af_core::error::DETAIL_DATABASE_ERROR;

use super::{
    DETAIL_BUDGET_EXHAUSTED, DETAIL_GITHUB_RECONNECT, DETAIL_GRANT_REQUIRED,
    DETAIL_INTEGRATION_NOT_CONNECTED, DETAIL_MINT_FAILED, DETAIL_MINT_UNCONFIGURED,
    DETAIL_RENEWAL_NO_CREDITS, DETAIL_RUN_LOST, DETAIL_RUN_NOT_FOUND, DETAIL_STALE_FENCE,
    DETAIL_VAULT_DATA_INVALID, Error,
};

struct Expected {
    error: Error,
    code: ErrorCode,
    detail: &'static str,
    permanent: bool,
    rejected: bool,
    has_source: bool,
}

#[test]
fn run_and_memory_refusals_have_stable_wire_classification() {
    let errors = [
        expected(
            super::stale_fence(),
            error_code::RUN_STALE_FENCING_TOKEN,
            DETAIL_STALE_FENCE,
        ),
        expected(
            super::run_not_found(),
            error_code::RUN_NOT_FOUND,
            DETAIL_RUN_NOT_FOUND,
        ),
        expected(super::run_lost(), error_code::RUN_LOST, DETAIL_RUN_LOST),
        expected(
            super::run_cancelled(),
            error_code::RUN_CANCELLED,
            super::DETAIL_RUN_CANCELLED,
        ),
        expected(
            super::renewal_no_credits(),
            error_code::RUN_RENEWAL_NO_CREDITS,
            DETAIL_RENEWAL_NO_CREDITS,
        ),
        expected(
            super::budget_exhausted(),
            error_code::RUN_BUDGET_EXCEEDED,
            DETAIL_BUDGET_EXHAUSTED,
        ),
    ];

    for expected in &errors {
        assert_error(expected);
    }
}

#[test]
fn credential_refusals_have_stable_wire_classification() {
    for expected in mint_refusals().iter().chain(&message_refusals()) {
        assert_error(expected);
    }
}

/// The seven mint and grant refusals, each with the code and sentence a runner
/// matches on.
fn mint_refusals() -> [Expected; 7] {
    [
        expected(
            super::integration_not_connected(),
            error_code::CRED_INTEGRATION_NOT_CONNECTED,
            DETAIL_INTEGRATION_NOT_CONNECTED,
        ),
        expected(
            super::mint_unconfigured(),
            error_code::CRED_BROKER_NOT_CONFIGURED,
            DETAIL_MINT_UNCONFIGURED,
        ),
        expected(
            super::github_reconnect_required(),
            error_code::GH_RECONNECT_REQUIRED,
            DETAIL_GITHUB_RECONNECT,
        ),
        expected(
            super::github_mint_failed(),
            error_code::GH_MINT_FAILED,
            DETAIL_MINT_FAILED,
        ),
        expected(
            super::connector_reconnect_required(),
            error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
            super::DETAIL_CONNECTOR_RECONNECT,
        ),
        expected(
            super::connector_mint_failed(),
            error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
            super::DETAIL_CONNECTOR_MINT_FAILED,
        ),
        expected(
            super::grant_required(),
            error_code::GRANT_NOT_FOUND,
            DETAIL_GRANT_REQUIRED,
        ),
    ]
}

/// The message refusal, and the corrupt sequence that answers as a row
/// fault.
fn message_refusals() -> [Expected; 2] {
    [
        expected(
            super::message_no_channel(),
            error_code::MESSAGE_NO_CHANNEL,
            super::DETAIL_MESSAGE_NO_CHANNEL,
        ),
        expected(
            super::sequence_corrupt(),
            error_code::INTERNAL_DB_QUERY,
            DETAIL_DATABASE_ERROR,
        ),
    ]
}

#[test]
fn contextual_errors_retain_context_and_sources() -> Result<(), &'static str> {
    let rejected_detail = "workers must be positive";
    let mut rejected = expected(
        super::rejected(rejected_detail),
        error_code::INVALID_REQUEST,
        rejected_detail,
    );
    rejected.rejected = true;

    let mut query = expected(
        super::query("loading run")(sqlx::Error::Protocol("fixture query".into())),
        error_code::INTERNAL_DB_QUERY,
        DETAIL_DATABASE_ERROR,
    );
    query.has_source = true;

    let malformed_id = af_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed fixture unexpectedly parsed")?;
    let mut malformed = expected(
        super::row_malformed("runs", "fleet_id")(malformed_id),
        error_code::INTERNAL_DB_QUERY,
        DETAIL_DATABASE_ERROR,
    );
    malformed.has_source = true;

    for expected in &[rejected, query, malformed] {
        assert_error(expected);
    }

    Ok(())
}

#[test]
fn invalid_vault_data_is_the_only_payload_free_permanent_failure() {
    let mut invalid = expected(
        super::vault_data_invalid(),
        error_code::VAULT_DATA_INVALID,
        DETAIL_VAULT_DATA_INVALID,
    );
    invalid.permanent = true;
    assert_error(&invalid);
}

// Value: checkpoint refusals retain their shared code rather than becoming bad requests.
#[test]
fn context_refusals_retain_their_code_and_typed_cause() -> Result<(), &'static str> {
    let source = af_context::Error::stale();
    let code = source.code();
    let error = Error::from(source);
    assert_eq!(error.code(), code);
    assert!(!error.is_rejected());
    assert!(!error.is_datastore_unavailable());
    assert!(
        error
            .source()
            .ok_or("missing context cause")?
            .is::<af_context::Error>()
    );
    Ok(())
}

fn expected(error: Error, code: ErrorCode, detail: &'static str) -> Expected {
    Expected {
        error,
        code,
        detail,
        permanent: false,
        rejected: false,
        has_source: false,
    }
}

fn assert_error(expected: &Expected) {
    assert_eq!(expected.error.code(), expected.code);
    assert_eq!(expected.error.detail(), expected.detail);
    assert_eq!(expected.error.is_config_permanent(), expected.permanent);
    assert_eq!(expected.error.is_rejected(), expected.rejected);
    assert!(!expected.error.is_datastore_unavailable());
    assert_eq!(expected.error.source().is_some(), expected.has_source);
    assert!(expected.error.to_string().contains(expected.code.as_str()));
}

/// A fleet with no reviewed native setup is refused for good, with a sentence
/// that sends its owner to review it rather than to retry.
#[test]
fn a_fleet_without_a_reviewed_setup_is_told_to_review_it() {
    let missing = Error::from(super::ErrorKind::SetupUnavailable);

    assert_eq!(missing.code(), error_code::SETUP_POLICY);
    // pin test: literal is the contract
    assert_eq!(
        missing.detail(),
        "The fleet needs a reviewed native setup before it can run"
    );
    assert!(missing.is_config_permanent());
    assert!(!missing.is_datastore_unavailable());
}

#[test]
fn internal_counter_overflow_preserves_its_cause() -> super::Result<()> {
    for source in [i64::try_from(u64::MAX).err(), i32::try_from(u32::MAX).err()] {
        let source =
            source.ok_or_else(|| super::rejected("overflow fixture must fail conversion"))?;
        let corrupt = Error::from(super::ErrorKind::DurableTaskCounter {
            column: "generation",
            source,
        });
        assert_eq!(corrupt.code(), error_code::INTERNAL_DB_QUERY);
        assert_eq!(corrupt.detail(), DETAIL_DATABASE_ERROR);
        assert!(
            corrupt
                .source()
                .is_some_and(<dyn std::error::Error>::is::<std::num::TryFromIntError>)
        );
    }
    Ok(())
}

/// A deadline this process actually reached, as `tokio` reports one.
async fn elapsed() -> tokio::time::error::Elapsed {
    tokio::time::timeout(std::time::Duration::ZERO, std::future::pending::<()>())
        .await
        .expect_err("a pending future never beats a zero deadline")
}

/// A field one byte past its declared bound, so `garde` refuses it.
#[derive(garde::Validate)]
struct Bounded {
    #[garde(length(max = 1))]
    question: String,
}

// Value: protects=a decision whose request breaks its bounds, or whose call
// overruns its deadline, answers the decision-unavailable code and sentence and
// names which of the two it was; fails_when=either decision arm is regrouped
// under another code, sentence or observation reason; why_new=neither arm was
// classified by any test; seam=the `From` lifts the decision path raises them through.
#[tokio::test]
async fn a_decision_fault_answers_unavailable_and_names_its_reason() {
    let report = garde::Validate::validate(&Bounded {
        question: "ab".to_owned(),
    })
    .expect_err("two characters break a one-character bound");
    let bounds = Error::from(report);
    let deadline = Error::from(super::ErrorKind::DecisionDeadline {
        source: elapsed().await,
    });

    for (fault, reason) in [
        (&bounds, "request_bounds"),
        (&deadline, "decision_deadline"),
    ] {
        assert_eq!(fault.code(), error_code::DURABLE_TASK_DECISION_UNAVAILABLE);
        assert_eq!(
            fault.detail(),
            "the fleet cannot currently obtain a funded reviewed decision"
        );
        assert_eq!(fault.decision_failure_reason(), Some(reason));
        assert_eq!(fault.durable_task_timeout_reason(), None);
        assert!(!fault.is_datastore_unavailable());
        assert!(fault.source().is_some(), "{reason} keeps its cause");
    }
}

// Value: protects=an upload request that cannot be built is a storage outage,
// never a bad request; fails_when=the request-construction kind leaves the
// storage family; why_new=no test built one; seam=the `http::Error` lift.
#[test]
fn an_unbuildable_storage_request_answers_storage_unavailable() {
    let built = http::Request::builder().uri("http://bad host/").body(());
    let fault = Error::from(built.expect_err("a space is no authority"));

    assert_eq!(fault.code(), error_code::DURABLE_TASK_STORAGE_UNAVAILABLE);
    assert_eq!(
        fault.detail(),
        "checkpoint storage cannot currently establish object custody"
    );
    assert_eq!(fault.remote_cooldown(), None);
    assert!(!fault.is_datastore_unavailable());
    assert!(
        fault
            .source()
            .is_some_and(<dyn std::error::Error>::is::<http::Error>)
    );
}

// Value: protects=a provider cooldown carries the instant work may resume and
// answers effect-uncertain; fails_when=the cooldown's instant is dropped or its
// code moves; why_new=no test read a cooldown back through the error; seam=the
// kind the GitHub client raises on a rate limit.
#[test]
fn a_remote_cooldown_carries_its_resume_instant() {
    let until = jiff::Timestamp::from_second(1_800_000_000).expect("a representable instant");
    let fault = Error::from(super::ErrorKind::DurableTaskRemoteCooldown { status: 429, until });

    assert_eq!(fault.remote_cooldown(), Some(until));
    assert_eq!(fault.code(), error_code::DURABLE_TASK_EFFECT_UNCERTAIN);
    assert_eq!(
        fault.detail(),
        "the saved publication requires authoritative reconciliation"
    );
    assert_eq!(fault.durable_task_timeout_reason(), None);
    assert!(fault.source().is_none());
}

// Value: protects=a GitHub request that times out is observed as a remote
// deadline, and one refused outright is not; fails_when=the timeout guard is
// dropped or inverted; why_new=no test raised a timed-out remote request;
// seam=a real `reqwest` client against a loopback listener that never answers.
#[tokio::test]
async fn a_remote_request_that_times_out_is_a_remote_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener binds");
    let address = listener.local_addr().expect("the listener has an address");
    let client = afd_identity::direct::direct(std::time::Duration::from_millis(50))
        .build()
        .expect("a client builds");

    let silent = client
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect_err("a listener that never answers times the request out");
    let timed_out = Error::from(silent);
    drop(listener);
    let refused = client
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect_err("a closed port refuses the connection");
    let refused = Error::from(refused);

    assert_eq!(
        timed_out.durable_task_timeout_reason(),
        Some("remote_deadline")
    );
    assert_eq!(refused.durable_task_timeout_reason(), None);
    for fault in [&timed_out, &refused] {
        assert_eq!(fault.code(), error_code::DURABLE_TASK_EFFECT_UNCERTAIN);
        assert_eq!(fault.remote_cooldown(), None);
        assert!(!fault.is_datastore_unavailable());
    }
}

// Value: protects=a task mutation refused because its datastore read ran out of
// time is reported as an outage, with the inner fault's code; fails_when=the
// mutation arm stops asking its source; why_new=no test wrapped an unavailable
// fault in a mutation; seam=the kind the adoption path wraps its faults in.
#[tokio::test]
async fn a_mutation_over_an_unavailable_read_is_an_outage() {
    let read = Error::from(elapsed().await);
    let unavailable = Error::from(super::ErrorKind::DurableTaskMutation {
        state: af_wire::durable_task::DurableTaskState::Running,
        source: Box::new(read),
    });
    let refused = Error::from(super::ErrorKind::DurableTaskMutation {
        state: af_wire::durable_task::DurableTaskState::Running,
        source: Box::new(super::run_lost()),
    });

    assert!(unavailable.is_datastore_unavailable());
    assert_eq!(unavailable.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(!refused.is_datastore_unavailable());
    assert_eq!(refused.code(), error_code::RUN_LOST);
    assert_eq!(refused.detail(), DETAIL_RUN_LOST);
}
