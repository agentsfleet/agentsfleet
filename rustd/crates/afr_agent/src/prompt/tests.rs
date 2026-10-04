use std::borrow::Cow;

use afd_wire::lease::LeasePayload;
use afd_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
use afd_wire::policy::{
    HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
    RepositoryAccess, RepositoryBinding,
};

use super::Prompt;
use crate::fixture::{lease, unbounded};

/// The bound repository, as the `ci-repairer` fixture names it.
const REPOSITORY: &str = "agentsfleet/linkwarden";
/// A repository the binding does not name.
const ELSEWHERE: &str = "agentsfleet/elsewhere";
/// The branch the daemon named for this lease.
const BRANCH: &str = "agentsfleet-repair/run-41";
/// The binding's base.
const BASE: &str = "dev";
/// The heading a write-bound lease's system prompt carries.
const REPAIR_HEADING: &str = "## Trusted repair context";

/// A lease bound to [`REPOSITORY`] with `access`, carrying `rules`.
fn bound(access: RepositoryAccess, rules: Vec<HttpRequestRule<'static>>) -> LeasePayload<'static> {
    let mut lease = lease(&[], unbounded());
    lease.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access,
        base_branch: BASE.into(),
    });
    lease.policy.http_origin_policies = vec![HttpOriginPolicy {
        host: "api.github.com".into(),
        credential_names: vec!["github".into()],
        requests: rules,
    }];
    lease
}

/// The locked rule the daemon compiles for a write binding's one ref.
fn locked_ref(repository_name: &str) -> HttpRequestRule<'static> {
    HttpRequestRule {
        method: HttpMethod::Post,
        path: repository::path(repository_name, REFS_PATH).into(),
        path_match: HttpPathMatch::Exact,
        json_fields: vec![HttpJsonFieldRule {
            name: FIELD_REF.into(),
            string_value: Some(format!("{REFS_HEADS}{BRANCH}").into()),
            boolean_value: None,
        }],
    }
}

#[test]
fn should_ask_the_events_message_under_the_installed_instructions() {
    let prompt = Prompt::new(&lease(&[], unbounded()));

    assert_eq!(prompt.message, "triage the failed run");
    assert_eq!(
        prompt.instructions,
        "## Installed instructions\n\nRead the run."
    );
}

#[test]
fn should_ask_the_whole_request_when_it_carries_no_message_string() {
    for request in [
        "not json",
        "{\"message\": 7}",
        "{\"other\": \"x\"}",
        "[\"message\"]",
    ] {
        let mut lease = lease(&[], unbounded());
        lease.event.request_json = Cow::Borrowed(request);

        assert_eq!(Prompt::new(&lease).message, request);
    }
}

#[test]
fn should_send_no_system_prompt_without_instructions() {
    let mut lease = lease(&[], unbounded());
    lease.instructions = Cow::Borrowed("");

    assert_eq!(Prompt::new(&lease).instructions, "");
}

#[test]
fn test_prompt_carries_trusted_repair_context() {
    let written = Prompt::new(&bound(
        RepositoryAccess::Write,
        vec![locked_ref(REPOSITORY)],
    ));
    assert_eq!(
        written.instructions,
        "## Installed instructions\n\nRead the run.\n\n## Trusted repair context\n\
         repository: agentsfleet/linkwarden\nrepair branch: agentsfleet-repair/run-41\n\
         trusted base: dev"
    );

    let read = Prompt::new(&bound(RepositoryAccess::Read, vec![locked_ref(REPOSITORY)]));
    assert_eq!(
        read.instructions,
        "## Installed instructions\n\nRead the run."
    );
}

#[test]
fn should_render_no_repair_context_without_a_ref_rule_for_the_bound_repository() {
    for rules in [Vec::new(), vec![locked_ref(ELSEWHERE)]] {
        let prompt = Prompt::new(&bound(RepositoryAccess::Write, rules));

        assert!(!prompt.instructions.contains(REPAIR_HEADING));
    }
}

#[test]
fn should_render_the_repair_context_alone_without_instructions() {
    let mut lease = bound(RepositoryAccess::Write, vec![locked_ref(REPOSITORY)]);
    lease.instructions = Cow::Borrowed("");

    assert!(Prompt::new(&lease).instructions.starts_with(REPAIR_HEADING));
}

/// The branch a repair may push names one repository, so a write binding naming
/// none or several authorises no repair, even with the ref rule locked.
#[test]
fn should_render_no_repair_context_for_a_write_binding_not_naming_one_repository() {
    for repositories in [Vec::new(), vec![REPOSITORY.into(), ELSEWHERE.into()]] {
        let mut lease = bound(RepositoryAccess::Write, vec![locked_ref(REPOSITORY)]);
        lease.policy.repository_binding = Some(RepositoryBinding {
            repositories,
            access: RepositoryAccess::Write,
            base_branch: BASE.into(),
        });

        assert!(!Prompt::new(&lease).instructions.contains(REPAIR_HEADING));
    }
}
