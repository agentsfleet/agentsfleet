use super::*;

#[test]
fn fleet_conflicts_distinguish_version_model_and_purge() {
    for (code, status, instruction) in [
        (error_code::SETUP_PRECONDITION, 428, "If-Match"),
        (error_code::SETUP_STALE, 412, "ETag"),
        (error_code::SETUP_MODEL, 409, "workspace owner"),
        (error_code::SETUP_GONE, 410, "deleted"),
    ] {
        let problem = Problem::of(code);
        assert_eq!(problem.status(), status, "{}", code.as_str());
        assert!(problem.hint().contains(instruction), "{problem:?}");
        assert!(problem.user_message().is_some());
    }
}
