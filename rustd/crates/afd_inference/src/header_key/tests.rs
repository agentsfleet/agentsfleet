#![expect(clippy::unwrap_used, reason = "fixtures fail loudly on invalid setup")]

use super::{KEY_HEADER, move_key};
use rig_core::wire::Body;

/// A key no test output may ever contain.
const SECRET: &str = "AIza-never-in-a-url";

fn request(uri: &str) -> http::Request<Body> {
    http::Request::post(uri).body(Body::empty()).unwrap()
}

#[test]
fn a_key_in_the_query_moves_to_a_sensitive_header_and_the_rest_of_the_query_stays() {
    let mut moved = request(&format!(
        "https://gemini.example/v1beta/models/m:streamGenerateContent?alt=sse&key={SECRET}"
    ));
    move_key(&mut moved).unwrap();
    assert_eq!(moved.uri().query(), Some("alt=sse"));
    assert_eq!(moved.uri().path(), "/v1beta/models/m:streamGenerateContent");
    let header = moved.headers().get(KEY_HEADER).unwrap();
    assert_eq!(header.to_str().unwrap(), SECRET);
    assert!(header.is_sensitive(), "a header dump must redact the key");
    assert!(!format!("{moved:?}").contains(SECRET));
    assert!(!moved.uri().to_string().contains(SECRET));
}

#[test]
fn a_key_that_was_the_only_parameter_leaves_no_query_behind() {
    let mut moved = request(&format!(
        "https://gemini.example/v1beta/models/m:generateContent?key={SECRET}"
    ));
    move_key(&mut moved).unwrap();
    assert_eq!(moved.uri().query(), None);
    assert!(moved.headers().contains_key(KEY_HEADER));
}

#[test]
fn a_request_without_a_key_is_left_exactly_as_it_was() {
    for uri in [
        "https://gemini.example/v1beta/models/m:generateContent",
        "https://gemini.example/v1beta/models/m:streamGenerateContent?alt=sse",
    ] {
        let mut untouched = request(uri);
        move_key(&mut untouched).unwrap();
        assert_eq!(untouched.uri().to_string(), uri);
        assert!(!untouched.headers().contains_key(KEY_HEADER));
    }
}

#[test]
fn a_key_that_cannot_be_a_header_is_refused_without_repeating_it() {
    // `%0A` decodes to a newline, which no header value may hold.
    let mut refused = request(&format!(
        "https://gemini.example/v1beta/models/m:generateContent?key={SECRET}%0A"
    ));
    let error = move_key(&mut refused).unwrap_err();
    assert!(!error.to_string().contains(SECRET));
    assert!(!format!("{error:?}").contains(SECRET));
    let provider = rig_core::ProviderError::from(error);
    assert!(!provider.to_string().contains(SECRET));
    assert!(!format!("{provider:?}").contains(SECRET));
}
