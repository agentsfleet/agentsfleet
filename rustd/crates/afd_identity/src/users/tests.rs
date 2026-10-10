#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_auth::principal::Subject;

use super::UsersEndpoint;

/// The metadata write's tail, as the writer passes it.
const METADATA: &str = "metadata";

fn endpoint(base: &str) -> UsersEndpoint {
    UsersEndpoint::parse(base).expect("a base a person can be appended to")
}

fn person(base: &str, subject: &str, tail: &[&str]) -> Option<String> {
    let subject = Subject::new(subject).expect("a non-blank subject");
    endpoint(base)
        .person(&subject, tail)
        .map(|url| url.as_str().to_owned())
}

#[test]
fn a_provider_shaped_subject_addresses_the_same_url_as_before() {
    // pin test: literal is the contract
    assert_eq!(
        person("https://api.clerk.com/v1", "user_2aXyTest", &[]).as_deref(),
        Some("https://api.clerk.com/v1/users/user_2aXyTest")
    );
    // pin test: literal is the contract
    assert_eq!(
        person("http://127.0.0.1:9931", "user_2aXyTest", &[METADATA]).as_deref(),
        Some("http://127.0.0.1:9931/users/user_2aXyTest/metadata")
    );
}

#[test]
fn a_base_with_a_trailing_slash_keeps_it_as_written() {
    // pin test: literal is the contract
    assert_eq!(
        person("https://api.clerk.com/v1/", "user_x", &[]).as_deref(),
        Some("https://api.clerk.com/v1//users/user_x")
    );
}

#[test]
fn a_subject_with_url_syntax_stays_one_escaped_segment() {
    // pin test: literal is the contract
    assert_eq!(
        person("https://h.test", "a/b?c#d%e", &[METADATA]).as_deref(),
        Some("https://h.test/users/a%2Fb%3Fc%23d%25e/metadata")
    );
}

#[test]
fn a_dot_segment_subject_names_no_person() {
    for dots in [".", ".."] {
        assert_eq!(person("https://h.test", dots, &[]), None, "{dots}");
        assert_eq!(person("https://h.test", dots, &[METADATA]), None, "{dots}");
    }
}

#[test]
fn a_base_that_is_not_a_hierarchical_url_is_refused() {
    for base in ["not a url", "mailto:ops@h.test"] {
        assert!(UsersEndpoint::parse(base).is_none(), "{base}");
    }
}
