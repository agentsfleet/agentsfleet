//! Each device-flow field at its bound, answering its own registry code.

use super::*;
use afd_core::error_code::{self, ErrorCode};

#[test]
fn two_broken_fields_answer_for_the_first_the_request_carries() {
    // garde reports every break; the table answers in request order, which is
    // the order the hand-written checks ran in before the bounds moved here.
    let over_nonce = "n".repeat(NONCE_MAX + 1);
    assert_eq!(
        refusal(Approval::parse("k", "", &over_nonce, "abc")),
        Some(error_code::INVALID_CIPHERTEXT)
    );
    let over_key = "k".repeat(PUBLIC_KEY_MAX + 1);
    assert_eq!(
        refusal(Opening::parse(&over_key, "lap\ntop")),
        Some(error_code::INVALID_PUBLIC_KEY)
    );
    assert_eq!(
        refusal(Opening::parse("key", &"t".repeat(TOKEN_NAME_MAX + 1))),
        Some(error_code::INVALID_TOKEN_NAME)
    );
}

/// The code a refusal carries, or `None` when the value parsed.
///
/// Written this way rather than with `expect_err` because the workspace
/// denies panicking helpers even in tests: an assertion that reads as a
/// comparison fails with both values printed, where an `expect` fails with
/// a message somebody wrote in advance.
fn refusal<T>(result: Result<T>) -> Option<ErrorCode> {
    result.err().map(|error| error.code())
}

#[test]
fn an_empty_field_and_an_oversized_one_answer_one_code() {
    let long = "k".repeat(PUBLIC_KEY_MAX + 1);
    for value in ["", long.as_str()] {
        assert_eq!(
            refusal(Opening::parse(value, "laptop")),
            Some(error_code::INVALID_PUBLIC_KEY),
            "public key {:?}",
            value.len()
        );
    }
}

#[test]
fn a_token_name_outside_printable_ascii_is_refused() {
    assert_eq!(
        refusal(Opening::parse("key", "lap\ntop")),
        Some(error_code::INVALID_TOKEN_NAME)
    );
    assert_eq!(refusal(Opening::parse("key", "Indy's laptop ~ 2")), None);
}

#[test]
fn a_code_is_six_digits_and_nothing_else() {
    assert_eq!(Code::parse("012345").map(Code::as_str).ok(), Some("012345"));
    // The last is six Arabic-Indic digits: `char::is_numeric` would accept
    // them, `is_ascii_digit` does not, and the store's Lua compares bytes.
    for bad in ["", "12345", "1234567", "12345a", "12345 ", "١٢٣٤٥٦"] {
        assert_eq!(
            refusal(Code::parse(bad)),
            Some(error_code::INVALID_VERIFICATION_CODE),
            "code {bad:?}"
        );
    }
}

#[test]
fn each_approval_field_answers_its_own_code() {
    let over_ciphertext = "c".repeat(CIPHERTEXT_MAX + 1);
    let over_nonce = "n".repeat(NONCE_MAX + 1);
    let cases = [
        ("", "c", "n", "012345", error_code::INVALID_PUBLIC_KEY),
        ("k", "", "n", "012345", error_code::INVALID_CIPHERTEXT),
        (
            "k",
            over_ciphertext.as_str(),
            "n",
            "012345",
            error_code::INVALID_CIPHERTEXT,
        ),
        ("k", "c", "", "012345", error_code::INVALID_NONCE),
        (
            "k",
            "c",
            over_nonce.as_str(),
            "012345",
            error_code::INVALID_NONCE,
        ),
        ("k", "c", "n", "abc", error_code::INVALID_VERIFICATION_CODE),
    ];
    for (key, ciphertext, nonce, code, expected) in cases {
        assert_eq!(
            refusal(Approval::parse(key, ciphertext, nonce, code)),
            Some(expected),
            "approval with code {code:?}"
        );
    }
    assert_eq!(refusal(Approval::parse("k", "c", "n", "012345")), None);
}
