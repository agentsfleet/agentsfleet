//! The policy every egress suite admits under, this crate's and the tools'.
//!
//! Shaped like the `ci-repairer` bundle's lease: GitHub minted and bound by
//! origin rules that lock the repair branch and the draft, Grafana and Elastic
//! static with their own hosts, an Elastic query path `read_only` admits, and
//! the Pushover credential its handler reads for itself. Built from the wire's
//! own types, so a renamed field fails to compile rather than to parse.

use std::borrow::Cow;

use afd_wire::policy::repository::{
    self, FIELD_BASE, FIELD_DRAFT, FIELD_HEAD, FIELD_REF, PULLS_PATH, REFS_HEADS, REFS_PATH,
};
use afd_wire::policy::{
    ContextBudget, ExecutionPolicy, HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch,
    HttpRequestRule, Mintable, NetworkPolicy, RepositoryAccess, RepositoryBinding,
};
use afr_secrets::FIELD_HOST;
use serde_json::{Map, Value};

use crate::error::Error;
use crate::refusal::Refusal;

/// The lease every suite's guard belongs to.
pub const LEASE_ID: &str = "lease-1";
/// The bound repository.
pub const REPOSITORY: &str = "acme/widgets";
/// The branch the daemon named.
pub const BRANCH: &str = "agentsfleet-repair/run-41";
/// The binding's base.
pub const BASE: &str = "dev";
/// GitHub's API host.
pub const GITHUB: &str = "api.github.com";
/// The Grafana host, which its static credential names.
pub const GRAFANA: &str = "demo-grafana.internal";
/// The Elastic host.
pub const ELASTIC: &str = "demo.es.example";
/// The Grafana token.
pub const GRAFANA_TOKEN: &str = "glsa_live_token";
/// The Elastic query path `read_only` admits a `POST` to.
pub const ELASTIC_QUERY: &str = "https://demo.es.example/_query";
/// Pushover's host.
pub const PUSHOVER: &str = "api.pushover.net";
/// The Pushover application token.
pub const PUSHOVER_TOKEN: &str = "po_app_token";
/// The Pushover recipient.
pub const PUSHOVER_USER: &str = "po_user_key";

/// The minted credential, and the integration it is minted for.
const GITHUB_CREDENTIAL: &str = "github";
/// The field a credential's token is held under.
const TOKEN: &str = "token";
/// An address literal the allowlist lists, which admission still refuses.
const LOOPBACK: &str = "127.0.0.1";

/// The policy, with `read_only` as given.
#[must_use]
pub fn policy(read_only: bool) -> ExecutionPolicy<'static> {
    ExecutionPolicy {
        network_policy: NetworkPolicy {
            allow: [GITHUB, GRAFANA, ELASTIC, PUSHOVER, LOOPBACK]
                .map(Cow::Borrowed)
                .into(),
            read_only,
            read_post_paths: vec![Cow::Borrowed(ELASTIC_QUERY)],
        },
        tools: Vec::new(),
        secrets_map: Some(secrets()),
        mintable: vec![Mintable {
            name: GITHUB_CREDENTIAL.into(),
            integration: GITHUB_CREDENTIAL.into(),
        }],
        provider: "anthropic".into(),
        api_key: "sk-test".into(),
        inference_host: "api.anthropic.com".into(),
        base_url: None,
        repository_binding: Some(RepositoryBinding {
            repositories: vec![REPOSITORY.into()],
            access: RepositoryAccess::Write,
            base_branch: BASE.into(),
        }),
        http_origin_policies: vec![HttpOriginPolicy {
            host: GITHUB.into(),
            credential_names: vec![GITHUB_CREDENTIAL.into()],
            requests: github_rules(),
        }],
        context: ContextBudget {
            tool_window: 8,
            memory_checkpoint_every: 0,
            stage_chunk_threshold: 0.8,
            model: "m".into(),
            context_cap_tokens: 0,
        },
    }
}

/// The rules the daemon compiles for a write binding: read the repository,
/// create the one named ref, open one draft against the base.
fn github_rules() -> Vec<HttpRequestRule<'static>> {
    vec![
        rule(HttpMethod::Get, "/", HttpPathMatch::Prefix, Vec::new()),
        rule(
            HttpMethod::Post,
            REFS_PATH,
            HttpPathMatch::Exact,
            vec![locked(
                FIELD_REF,
                Some(format!("{REFS_HEADS}{BRANCH}")),
                None,
            )],
        ),
        rule(
            HttpMethod::Post,
            PULLS_PATH,
            HttpPathMatch::Exact,
            vec![
                locked(FIELD_HEAD, Some(BRANCH.to_owned()), None),
                locked(FIELD_BASE, Some(BASE.to_owned()), None),
                locked(FIELD_DRAFT, None, Some(true)),
            ],
        ),
    ]
}

fn rule(
    method: HttpMethod,
    suffix: &str,
    path_match: HttpPathMatch,
    json_fields: Vec<HttpJsonFieldRule<'static>>,
) -> HttpRequestRule<'static> {
    HttpRequestRule {
        method,
        path: repository::path(REPOSITORY, suffix).into(),
        path_match,
        json_fields,
    }
}

fn locked(
    name: &'static str,
    text: Option<String>,
    flag: Option<bool>,
) -> HttpJsonFieldRule<'static> {
    HttpJsonFieldRule {
        name: name.into(),
        string_value: text.map(Cow::Owned),
        boolean_value: flag,
    }
}

/// `secrets_map`: Grafana and Elastic bound to their hosts, one credential
/// bound to none, and Pushover's two fields.
fn secrets() -> Value {
    let credentials = [
        (
            "grafana",
            credential(&[(TOKEN, GRAFANA_TOKEN), (FIELD_HOST, GRAFANA)]),
        ),
        (
            "elastic",
            credential(&[("api_key", "es_live_key"), (FIELD_HOST, ELASTIC)]),
        ),
        ("unbound", credential(&[(TOKEN, "no_host_token")])),
        (
            "pushover",
            credential(&[(TOKEN, PUSHOVER_TOKEN), ("user", PUSHOVER_USER)]),
        ),
    ];
    Value::Object(
        credentials
            .into_iter()
            .map(|(name, fields)| (name.to_owned(), fields))
            .collect(),
    )
}

fn credential(fields: &[(&str, &str)]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|(field, value)| ((*field).to_owned(), Value::from(*value)))
            .collect::<Map<String, Value>>(),
    )
}

/// A refusal as a suite compares it: which one it is, and the sentence the
/// model reads.
pub type Shown = (Option<Refusal>, String);

/// How `error` shows to the model.
#[must_use]
pub fn shown(error: &Error) -> Shown {
    (error.refusal(), error.detail())
}
