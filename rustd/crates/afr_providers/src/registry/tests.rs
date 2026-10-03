#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::collections::HashSet;

use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use rig_core::providers::openai::wire::by_name;

use super::{BUILTIN, HTTPS, ProviderSpec, Registry, Wire};

/// A self-hosted endpoint a `custom:` provider names.
const CUSTOM_BASE: &str = "https://vllm.corp/v1";

fn builtin_specs() -> Vec<ProviderSpec> {
    serde_json::from_str(BUILTIN).unwrap()
}

#[test]
fn should_ship_a_table_whose_every_name_is_unique_and_dialled_over_https() {
    let specs = builtin_specs();
    let mut seen = HashSet::new();

    for spec in &specs {
        for name in spec.aliases.iter().chain([&spec.name]) {
            assert!(seen.insert(name.clone()), "{name} is named twice");
        }
        let base = reqwest::Url::parse(&spec.base_url).unwrap();
        assert_eq!(base.scheme(), HTTPS, "{}", spec.name);
        assert!(
            base.host_str().is_some_and(|host| host != "localhost"),
            "{}",
            spec.name
        );
    }
    assert!(
        seen.len() > 50,
        "every name the Zig runner speaks over these wires"
    );
}

#[test]
fn should_route_each_named_provider_to_its_wire_and_base() {
    let registry = Registry::builtin().unwrap();
    let route = |name: &str| registry.route(name).unwrap();

    assert_eq!(route("anthropic").wire, Wire::Messages);
    assert_eq!(route("openai").wire, Wire::Responses);
    for (name, base) in [
        ("groq", "https://api.groq.com/openai/v1"),
        ("mistral", "https://api.mistral.ai/"),
        ("deepseek", "https://api.deepseek.com/"),
        ("openrouter", "https://openrouter.ai/api/v1"),
    ] {
        let named = route(name);
        assert_eq!(
            (named.wire, named.base.as_str()),
            (Wire::Chat, base),
            "{name}"
        );
    }
    assert_eq!(route("grok"), route("xai"), "an alias is the same route");
}

#[test]
fn should_refuse_a_name_the_table_leaves_out() {
    let registry = Registry::builtin().unwrap();

    for left_out in [
        "bedrock",
        "glm",
        "minimax",
        "lmstudio",
        "copilot",
        "Anthropic",
    ] {
        let refused = registry.route(left_out).unwrap_err();
        assert_eq!(refused.unhosted_provider(), Some(left_out));
    }
}

#[test]
fn should_take_a_custom_url_only_over_https_with_a_host() {
    let registry = Registry::new([]).unwrap();
    let custom = |base: &str| format!("{CUSTOM_PROVIDER_PREFIX}{base}");

    let route = registry.route(&custom(CUSTOM_BASE)).unwrap();

    assert_eq!((route.wire, route.base.as_str()), (Wire::Chat, CUSTOM_BASE));
    for refused in ["http://vllm.corp/v1", "not a url", "https://", ""] {
        assert!(registry.route(&custom(refused)).is_err(), "{refused}");
    }
}

#[test]
fn should_dial_a_custom_endpoint_where_rig_appends_the_chat_path() {
    let registry = Registry::new([]).unwrap();
    let base = |url: &str| {
        let route = registry
            .route(&format!("{CUSTOM_PROVIDER_PREFIX}{url}"))
            .unwrap();
        route.base.as_str().trim_end_matches('/').to_owned()
    };

    for (written, dialled) in [
        ("https://vllm.corp/v1", "https://vllm.corp/v1"),
        ("https://vllm.corp/v1/", "https://vllm.corp/v1"),
        ("https://llm.acme.com", "https://llm.acme.com/v1"),
        ("https://llm.acme.com/", "https://llm.acme.com/v1"),
        (
            "https://vllm.corp/v1/chat/completions",
            "https://vllm.corp/v1",
        ),
        (
            "https://gateway.corp/chat/completions",
            "https://gateway.corp",
        ),
        (
            "https://ark.volces.com/api/coding/v3",
            "https://ark.volces.com/api/coding/v3",
        ),
    ] {
        assert_eq!(base(written), dialled, "{written}");
    }
}

#[test]
fn should_name_an_entry_whose_base_does_not_parse() {
    let broken = ProviderSpec {
        name: "broken".to_owned(),
        aliases: Vec::new(),
        wire: Wire::Chat,
        base_url: "not a url".to_owned(),
        dialect: None,
    };

    let failure = Registry::new([broken]).unwrap_err();

    assert!(failure.detail().contains("broken"), "{}", failure.detail());
}

// rig's dialect table is compiled into the library, so an upgrade that moves
// a vendor's `/v1` between its base and its path shows up here, before a turn
// posts to a doubled or missing segment. The host may differ: a regional
// entry (moonshot's `.cn`) speaks the global dialect at its own host.
#[test]
fn should_name_only_dialects_rig_knows_under_the_path_rig_joins_to() {
    let specs = builtin_specs();
    let named: Vec<&ProviderSpec> = specs.iter().filter(|spec| spec.dialect.is_some()).collect();

    for spec in &named {
        let name = spec.dialect.as_deref().unwrap();
        let dialect = by_name(name).unwrap_or_else(|| panic!("{name} is no rig dialect"));
        let ours = reqwest::Url::parse(&spec.base_url).unwrap();
        let rigs = reqwest::Url::parse(dialect.base_url).unwrap();
        assert_eq!(spec.wire, Wire::Chat, "{}", spec.name);
        assert_eq!(ours.path(), rigs.path(), "{}", spec.name);
    }
    assert!(named.len() >= 10, "every vendor rig has quirks for");
}

#[test]
fn should_keep_a_vendor_whose_dialect_drops_tools_a_plain_gateway() {
    let registry = Registry::builtin().unwrap();

    let perplexity = registry.route("perplexity").unwrap();
    let rigs = by_name("perplexity").unwrap();

    assert!(!rigs.quirks.supports_tools, "the reason it stays a gateway");
    assert_eq!(perplexity.dialect, None);
}
