use super::*;

#[test]
fn setup_failures_distinguish_input_version_and_uncertain_provider_outcomes() {
    for (code, status, instruction) in [
        (error_code::SETUP_TOKEN, 422, "Reduce context"),
        (error_code::SETUP_PRECONDITION, 428, "If-Match"),
        (error_code::SETUP_STALE, 412, "ETag"),
        (error_code::SETUP_UNKNOWN, 409, "Do not dispatch again"),
        (error_code::SETUP_PROVIDER, 502, "known usage"),
        (error_code::SETUP_MODEL, 409, "workspace owner"),
        (error_code::SETUP_BALANCE, 402, "wallet"),
        (error_code::SETUP_GONE, 410, "deleted"),
    ] {
        let problem = Problem::of(code);
        assert_eq!(problem.status(), status, "{}", code.as_str());
        assert!(problem.hint().contains(instruction), "{problem:?}");
        assert!(problem.user_message().is_some());
    }
}
