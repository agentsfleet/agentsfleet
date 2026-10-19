use super::*;

/// The statuses the auth plane depends on, stated rather than inferred.
///
/// These four are load-bearing beyond the envelope: `docs/AUTH.md` rests on
/// 022 being a 403 (re-authenticating cannot help), and the runner client
/// classifies 004 as transport loss rather than an auth rejection — which is
/// what stops a datastore outage walking a healthy fleet to shutdown.
#[test]
fn test_the_auth_planes_statuses_are_the_documented_ones() {
    for (code, status) in [
        (error_code::AUTH_INSUFFICIENT_SCOPE, 403),
        (error_code::AUTH_UNAUTHORIZED, 401),
        (error_code::AUTH_TOKEN_EXPIRED, 401),
        (error_code::AUTH_UNAVAILABLE, 503),
        (error_code::AUTH_CLI_CREDENTIAL_REVOKED, 401),
        (error_code::APIKEY_REVOKED, 401),
        (error_code::RUN_INVALID_RUNNER_TOKEN, 401),
        (error_code::RUN_ADMIN_STATE_BLOCKED, 401),
    ] {
        assert_eq!(Problem::of(code).status(), status, "{}", code.as_str());
    }
}

/// A reused steer operation id answers 409 with prose that names the field.
///
/// The status is the claim: a 409 is "the state refuses you, stop retrying",
/// and a steer client that retried a conflict would never succeed.
#[test]
fn test_operation_conflict_code_registered() {
    let problem = Problem::of(error_code::AGENTSFLEET_OPERATION_CONFLICT);
    // pin test: literal is the contract
    assert_eq!(problem.code().as_str(), "UZ-AGT-016");
    assert_eq!(problem.status(), 409);
    assert_ne!(problem.title(), "");
    assert!(
        problem.hint().contains("operation_id"),
        "{}",
        problem.hint()
    );
    assert!(problem.user_message().is_some());
    assert!(REGISTRY.contains(&error_code::AGENTSFLEET_OPERATION_CONFLICT));
}

/// A renewal refused for the retired cumulative shape answers 400, and its
/// hint names the field the runner should send instead.
///
/// The status is the claim: the runner ends a lease on any 4xx, and this code
/// is answered only for a body whose usage can never be recorded.
#[test]
fn test_renew_body_malformed_code_registered() {
    let problem = Problem::of(error_code::RUN_RENEW_BODY_MALFORMED);
    // pin test: literal is the contract
    assert_eq!(problem.code().as_str(), "UZ-RUN-013");
    assert_eq!(problem.status(), 400);
    assert_eq!(problem.title(), "Renew body malformed");
    assert!(problem.hint().contains("`requests`"), "{}", problem.hint());
}
