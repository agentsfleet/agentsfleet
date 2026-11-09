//! Every branch of the retired verify script, decided without a database.

use af_core::error::DETAIL_DATABASE_ERROR;
use af_core::error_code;
use af_crypto::mac::HmacSha256Tag;
use af_crypto::secret::SecretBytes;
use jiff::{SignedDuration, Timestamp};

use super::{Locked, MAX_VERIFY_ATTEMPTS, Write, classify};
use crate::session::fingerprint::Fingerprint;
use crate::session::state::{AbortReason, SessionStatus, VerifyOutcome};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);

fn code(text: &str) -> HmacSha256Tag {
    HmacSha256Tag::compute_peppered(
        &SecretBytes::new(b"pepper".to_vec()),
        &[b"sid", text.as_bytes()],
    )
}

fn fingerprint(agent: &str) -> Fingerprint {
    Fingerprint::of("203.0.113.7", agent, "sid")
}

/// Asserts `classified` refused its row as unreadable: the internal database
/// code and its fixed sentence, never a verify outcome.
fn assert_unreadable(
    classified: crate::Result<(VerifyOutcome, Option<Write>)>,
    why: &str,
) -> TestResult {
    let error = classified.err().ok_or(why.to_owned())?;
    assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY, "{why}");
    assert_eq!(error.detail(), DETAIL_DATABASE_ERROR, "{why}");
    Ok(())
}

fn row(status: SessionStatus) -> Locked {
    Locked {
        status: status.as_str().to_owned(),
        dashboard_public_key: Some("dpk".to_owned()),
        ciphertext: Some("ct".to_owned()),
        nonce: Some("n".to_owned()),
        verification_code_hmac: Some(code("123456").as_bytes().to_vec()),
        verification_attempt_count: 0,
        aborted_reason: None,
        consumed_client_fingerprint: None,
        expires_at: NOW + SignedDuration::from_secs(300),
        replay_expires_at: None,
        now: NOW,
    }
}

#[test]
fn the_right_code_redeems_and_writes_the_consume() -> TestResult {
    let (outcome, write) = classify(
        &row(SessionStatus::VerificationPending),
        &code("123456"),
        &fingerprint("afc/1"),
    )?;
    let VerifyOutcome::Success(payload) = outcome else {
        return Err("the right code redeems".into());
    };
    assert_eq!(payload.ciphertext, "ct");
    assert_eq!(write, Some(Write::Consume));
    Ok(())
}

#[test]
fn wrong_codes_count_then_abort_at_the_cap() -> TestResult {
    let mut locked = row(SessionStatus::VerificationPending);
    for attempt in 1..MAX_VERIFY_ATTEMPTS {
        locked.verification_attempt_count = i16::from(attempt - 1);
        let (outcome, write) = classify(&locked, &code("000000"), &fingerprint("afc/1"))?;
        assert_eq!(outcome, VerifyOutcome::InvalidCode(attempt));
        assert_eq!(
            write,
            Some(Write::Attempt {
                attempts: attempt,
                status: SessionStatus::VerificationPending,
                reason: None,
            })
        );
    }
    locked.verification_attempt_count = i16::from(MAX_VERIFY_ATTEMPTS - 1);
    let (outcome, write) = classify(&locked, &code("000000"), &fingerprint("afc/1"))?;
    assert_eq!(
        outcome,
        VerifyOutcome::RateLimited,
        "the fifth wrong code locks out"
    );
    assert_eq!(
        write,
        Some(Write::Attempt {
            attempts: MAX_VERIFY_ATTEMPTS,
            status: SessionStatus::Aborted,
            reason: Some(AbortReason::RateLimitExceeded),
        })
    );
    Ok(())
}

#[test]
fn an_approved_row_without_a_whole_digest_is_unreadable() -> TestResult {
    let mut locked = row(SessionStatus::VerificationPending);
    locked.verification_code_hmac = None;
    assert_unreadable(
        classify(&locked, &code("123456"), &fingerprint("afc/1")),
        "an approved row without its digest is unreadable",
    )?;
    locked.verification_code_hmac = Some(vec![1, 2, 3]);
    assert_unreadable(
        classify(&locked, &code("123456"), &fingerprint("afc/1")),
        "a short digest is unreadable, not a wrong code",
    )
}

#[test]
fn an_attempt_count_a_verify_never_writes_is_unreadable() -> TestResult {
    for count in [-1, i16::from(MAX_VERIFY_ATTEMPTS), i16::MAX] {
        let mut locked = row(SessionStatus::VerificationPending);
        locked.verification_attempt_count = count;
        assert_unreadable(
            classify(&locked, &code("000000"), &fingerprint("afc/1")),
            &format!("a count of {count} on a pending row is unreadable"),
        )?;
    }
    Ok(())
}

#[test]
fn replay_answers_only_the_same_request_inside_the_window() -> TestResult {
    let mut locked = row(SessionStatus::Consumed);
    locked.consumed_client_fingerprint = Some(fingerprint("afc/1").as_bytes().to_vec());
    locked.replay_expires_at = Some((NOW + SignedDuration::from_secs(60)).into());
    let (outcome, write) = classify(&locked, &code("123456"), &fingerprint("afc/1"))?;
    assert!(matches!(outcome, VerifyOutcome::Replay(_)));
    assert_eq!(write, None, "a replay writes nothing");
    let (outcome, _) = classify(&locked, &code("123456"), &fingerprint("afc/2"))?;
    assert_eq!(
        outcome,
        VerifyOutcome::Consumed,
        "another fingerprint is refused"
    );
    locked.replay_expires_at = Some(NOW.into());
    let (outcome, _) = classify(&locked, &code("123456"), &fingerprint("afc/1"))?;
    assert_eq!(
        outcome,
        VerifyOutcome::Consumed,
        "a closed window is refused"
    );
    Ok(())
}

#[test]
fn a_session_past_its_deadline_is_expired_whatever_its_status() -> TestResult {
    for status in SessionStatus::ALL {
        let mut locked = row(status);
        locked.expires_at = NOW;
        let (outcome, write) = classify(&locked, &code("123456"), &fingerprint("afc/1"))?;
        assert_eq!(outcome, VerifyOutcome::Expired, "{status:?}");
        assert_eq!(write, None);
    }
    Ok(())
}

#[test]
fn terminal_and_unapproved_sessions_answer_without_writing() -> TestResult {
    let mut aborted = row(SessionStatus::Aborted);
    aborted.aborted_reason = Some("explicit_cancel".to_owned());
    let cases = [
        (row(SessionStatus::Pending), VerifyOutcome::NotApproved),
        (row(SessionStatus::Expired), VerifyOutcome::Expired),
        (
            aborted,
            VerifyOutcome::Aborted(Some(AbortReason::ExplicitCancel)),
        ),
        (row(SessionStatus::Aborted), VerifyOutcome::Aborted(None)),
    ];
    for (locked, expected) in cases {
        let (outcome, write) = classify(&locked, &code("123456"), &fingerprint("afc/1"))?;
        assert_eq!(outcome, expected);
        assert_eq!(write, None);
    }
    Ok(())
}

#[test]
fn a_row_this_daemon_never_writes_is_refused() -> TestResult {
    let mut unknown = row(SessionStatus::Pending);
    unknown.status = "verified".to_owned();
    assert_unreadable(
        classify(&unknown, &code("123456"), &fingerprint("afc/1")),
        "an unknown status is unreadable",
    )?;
    let mut hollow = row(SessionStatus::VerificationPending);
    hollow.ciphertext = None;
    assert_unreadable(
        classify(&hollow, &code("123456"), &fingerprint("afc/1")),
        "an approved row without its payload is unreadable",
    )
}
