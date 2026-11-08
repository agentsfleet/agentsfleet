use super::*;

#[test]
fn authoring_failures_distinguish_input_version_and_uncertain_provider_outcomes() {
    for (code, status, instruction) in [
        (error_code::AUTHORING_TOKEN, 422, "Reduce context"),
        (error_code::AUTHORING_PRECONDITION, 428, "If-Match"),
        (error_code::AUTHORING_STALE, 412, "ETag"),
        (error_code::AUTHORING_TIMEOUT, 504, "obligation"),
        (error_code::AUTHORING_UNKNOWN, 409, "Do not dispatch again"),
        (error_code::AUTHORING_PROVIDER, 502, "known usage"),
        (error_code::AUTHORING_MODEL, 409, "workspace owner"),
        (error_code::AUTHORING_BALANCE, 402, "wallet"),
        (error_code::AUTHORING_FENCE, 409, "Stop this attempt"),
        (error_code::AUTHORING_GONE, 410, "deleted"),
    ] {
        let problem = Problem::of(code);
        assert_eq!(problem.status(), status, "{}", code.as_str());
        assert!(problem.hint().contains(instruction), "{problem:?}");
        assert!(problem.user_message().is_some());
    }
}
