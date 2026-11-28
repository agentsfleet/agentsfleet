use std::borrow::Cow;

use af_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
use af_wire::policy::{
    HttpEgress, HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
    RepositoryAccess, RepositoryBinding,
};
use af_wire::run::RunPayload;

use afr_memory::Seed;
use afr_tools::Runtime;
use afr_tools::catalog::{FILE_READ, GIT, HTTP_REQUEST, published};

use super::Prompt;
use crate::fixture::{payload, unbounded};

mod pinned;
mod tasks;

/// The bound repository, as the `ci-repairer` fixture names it.
const REPOSITORY: &str = "agentsfleet/linkwarden";
/// A repository the binding does not name.
const ELSEWHERE: &str = "agentsfleet/elsewhere";
/// The branch the daemon named for this run.
const BRANCH: &str = "agentsfleet-repair/run-41";
/// The binding's base.
const BASE: &str = "dev";
/// The heading a write-bound run's system prompt carries.
const REPAIR_HEADING: &str = "## Trusted repair context";

/// A run bound to [`REPOSITORY`] with `access`, carrying `rules`.
fn bound(access: RepositoryAccess, rules: Vec<HttpRequestRule<'static>>) -> RunPayload<'static> {
    let mut run = payload(&[], unbounded());
    run.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access,
        base_branch: BASE.into(),
    });
    run.policy.http_origin_policies = vec![HttpOriginPolicy {
        host: "api.github.com".into(),
        credential_names: vec!["github".into()],
        requests: rules,
    }];
    run
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
        permitted_fields: None,
    }
}

/// The prompt for `run`, whose memory load carried nothing.
fn prompt(run: &RunPayload<'_>) -> Prompt {
    let sandbox = run
        .policy
        .tools
        .iter()
        .any(|tool| published(tool).is_some_and(|entry| entry.runtime() == Runtime::Sandbox));
    Prompt::new(run, &Seed::default(), sandbox, true)
}

#[test]
fn should_ask_the_events_message_under_the_fixed_text() {
    let prompt = prompt(&payload(&[], unbounded()));

    assert_eq!(prompt.message, "triage the failed run");
    assert!(prompt.instructions.starts_with(&prompt.fixed));
}

#[test]
fn a_runs_prompt_carries_the_turns_date_outside_the_digest() {
    let mut run = payload(&[], unbounded());
    run.event.created_at = "2026-11-26T09:00:00Z".parse().expect("a fixture instant");
    let today = prompt(&run);
    assert!(
        today
            .instructions
            .ends_with("Today is Thursday, November 26, 2026 (UTC).")
    );
    run.event.created_at = "2026-11-27T00:00:00Z".parse().expect("a fixture instant");
    let tomorrow = prompt(&run);
    assert!(
        tomorrow
            .instructions
            .ends_with("Today is Friday, November 27, 2026 (UTC).")
    );
    assert_eq!(today.fixed, tomorrow.fixed);
    assert!(!today.fixed.contains("Today is"));
}

#[test]
fn a_private_fleets_prompt_says_search_is_off() {
    let mut run = payload(&[], unbounded());
    run.policy.network_policy.http_egress = HttpEgress::Restricted;
    let restricted = prompt(&run);
    assert!(restricted.instructions.contains(
        "## Environment\nWeb search is off. A person can enable it with Allow all in Network."
    ));
    run.policy.network_policy.http_egress = HttpEgress::Public;
    let public = prompt(&run);
    assert!(
        public
            .instructions
            .contains("## Environment\nNetwork policy allows public web search.")
    );
    assert!(!public.instructions.contains("Web search is off"));
}

#[test]
fn a_run_with_a_sandbox_is_told_where_commands_run() {
    let shell = "The working directory is /workspace. Shell commands run through sh -c.";
    assert!(prompt(&offering(GIT.name())).instructions.contains(shell));
    assert!(
        !prompt(&offering(HTTP_REQUEST.name()))
            .instructions
            .contains(shell)
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
        let mut run = payload(&[], unbounded());
        run.event.request_json = Cow::Borrowed(request);

        assert_eq!(prompt(&run).message, request);
    }
}

#[test]
fn test_prompt_carries_trusted_repair_context() {
    let written = prompt(&bound(
        RepositoryAccess::Write,
        vec![locked_ref(REPOSITORY)],
    ));
    assert!(written.instructions.contains(
        "## Trusted repair context\n\
         repository: agentsfleet/linkwarden\nrepair branch: agentsfleet-repair/run-41\n\
         trusted base: dev"
    ));

    let read = prompt(&bound(RepositoryAccess::Read, vec![locked_ref(REPOSITORY)]));
    assert!(!read.instructions.contains(REPAIR_HEADING));
}

#[test]
fn should_render_no_repair_context_without_a_ref_rule_for_the_bound_repository() {
    let mut unpinned = locked_ref(REPOSITORY);
    for field in &mut unpinned.json_fields {
        field.string_value = None;
    }
    for rules in [Vec::new(), vec![locked_ref(ELSEWHERE)], vec![unpinned]] {
        let prompt = prompt(&bound(RepositoryAccess::Write, rules));

        assert!(!prompt.instructions.contains(REPAIR_HEADING));
    }
}

/// The branch a repair may push names one repository, so a write binding naming
/// none or several authorises no repair, even with the ref rule locked.
#[test]
fn should_render_no_repair_context_for_a_write_binding_not_naming_one_repository() {
    for repositories in [Vec::new(), vec![REPOSITORY.into(), ELSEWHERE.into()]] {
        let mut run = bound(RepositoryAccess::Write, vec![locked_ref(REPOSITORY)]);
        run.policy.repository_binding = Some(RepositoryBinding {
            repositories,
            access: RepositoryAccess::Write,
            base_branch: BASE.into(),
        });

        assert!(!prompt(&run).instructions.contains(REPAIR_HEADING));
    }
}

/// A run offering `tool`, bound to [`REPOSITORY`] for reading.
fn offering(tool: &str) -> RunPayload<'static> {
    let mut run = bound(RepositoryAccess::Read, Vec::new());
    run.policy.tools = vec![tool.to_owned().into()];
    run
}

#[test]
fn should_name_where_each_repository_is_checked_out_when_a_tool_runs_processes() {
    let prompt = prompt(&offering(GIT.name()));

    assert!(
        prompt.instructions.contains(
            "\n\n## Workspace\nagentsfleet/linkwarden is checked out at ./linkwarden on dev, \
             with origin set"
        ),
        "{}",
        prompt.instructions
    );
}

/// The daemon sends a read binding with no base; the supervisor checks out
/// the remote's default branch, and the prompt says so rather than naming
/// an empty branch.
#[test]
fn should_name_the_default_branch_for_a_binding_with_no_base() {
    let mut run = offering(GIT.name());
    run.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Read,
        base_branch: "".into(),
    });

    let prompt = prompt(&run);

    assert!(
        prompt.instructions.contains(
            "\n\n## Workspace\nagentsfleet/linkwarden is checked out at ./linkwarden on its \
             default branch, with origin set"
        ),
        "{}",
        prompt.instructions
    );
}

/// A run of file tools alone works in the workspace too, so it is told
/// where the repository is; one with no tool in the sandbox is not.
#[test]
fn should_name_the_checkout_for_any_sandbox_tool_and_none_without_one() {
    let file_tools = prompt(&offering(FILE_READ.name()));
    let supervisor_only = prompt(&offering(HTTP_REQUEST.name()));

    assert!(file_tools.instructions.contains("## Workspace"));
    assert!(!supervisor_only.instructions.contains("## Workspace"));
}

#[test]
fn should_name_no_checkout_for_a_binding_that_does_not_parse() {
    let mut run = offering(GIT.name());
    run.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec!["../escape".into()],
        access: RepositoryAccess::Read,
        base_branch: BASE.into(),
    });

    assert!(!prompt(&run).instructions.contains("## Workspace"));
}
