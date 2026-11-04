//! Per-call measurements and catalogue replacement on a real SDK reader.
use afd_observability::metrics::label::models::CallOutcome;
use afd_observability::producers::models::{self, ModelInfo};
use afd_observability::test_util::Capture;
use std::time::Duration;

fn model(name: &str) -> ModelInfo {
    ModelInfo {
        model: name.into(),
        provider: "openai".into(),
        context_tokens: 128_000,
        output_tokens: Some(4096),
        tools: Some(false),
        image_transport: Some(true),
    }
}
#[test]
fn should_export_calls_and_explicit_catalogue_values_without_stale_models() {
    models::call(
        "gpt-fixture",
        "openai",
        CallOutcome::Completed,
        Duration::ZERO,
        None,
    );
    models::catalogue(None);
    let captured = Capture::install();
    assert_eq!(
        captured.gauge("agentsfleet_model_catalog_snapshot_age_seconds", &[]),
        None
    );
    assert_call_outcomes(&captured);
    assert_token_observations(&captured);
    assert_hosted_provider(&captured);
    assert_catalogue_replacement(&captured);
    assert_catalogue_withdrawal(&captured);
}

fn assert_call_outcomes(captured: &Capture) {
    for outcome in CallOutcome::ALL {
        let labels = [
            ("gen_ai.request.model", "gpt-fixture"),
            ("gen_ai.provider.name", "openai"),
            ("outcome", outcome.as_str()),
        ];
        models::call(
            "gpt-fixture",
            "openai",
            *outcome,
            Duration::from_millis(700),
            (*outcome == CallOutcome::Completed).then_some(Duration::ZERO),
        );
        assert_eq!(
            captured.histogram_count("agentsfleet_model_call_duration_seconds", &labels),
            1
        );
        assert_eq!(
            captured.histogram_count("agentsfleet_model_first_token_duration_seconds", &labels),
            u64::from(*outcome == CallOutcome::Completed)
        );
    }
}

fn assert_hosted_provider(captured: &Capture) {
    let served_model = "accounts/fireworks/models/minimax-m3p1-flash";
    let served_labels = [
        ("gen_ai.request.model", served_model),
        ("gen_ai.provider.name", "fireworks"),
        ("outcome", "completed"),
    ];
    models::call(
        served_model,
        "Fireworks",
        CallOutcome::Completed,
        Duration::from_millis(20),
        Some(Duration::from_millis(5)),
    );
    assert_eq!(
        captured.histogram_count("agentsfleet_model_call_duration_seconds", &served_labels),
        1
    );
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_model_first_token_duration_seconds",
            &served_labels
        ),
        1
    );
}

fn assert_catalogue_replacement(captured: &Capture) {
    let labels = [
        ("gen_ai.request.model", "gpt-fixture"),
        ("gen_ai.provider.name", "openai"),
        ("capability", "tools"),
    ];
    models::catalogue(Some((vec![model("gpt-fixture")], false)));
    assert_eq!(
        captured.gauge("agentsfleet_model_route_capability", &labels),
        Some(0)
    );
    assert_eq!(
        captured.gauge(
            "agentsfleet_model_limit_tokens",
            &[
                ("gen_ai.request.model", "gpt-fixture"),
                ("gen_ai.provider.name", "openai"),
                ("limit", "context")
            ]
        ),
        Some(128_000)
    );
    let mut next = model("replacement");
    next.tools = None;
    next.output_tokens = None;
    models::catalogue(Some((vec![next], false)));
    assert_eq!(
        captured.gauge("agentsfleet_model_route_capability", &labels),
        None
    );
}

fn assert_catalogue_withdrawal(captured: &Capture) {
    let many = (0..=models::MAX_MODELS)
        .map(|n| model(&format!("model-{n}")))
        .collect();
    models::catalogue(Some((many, false)));
    assert_eq!(
        captured.gauge("agentsfleet_model_catalog_clipped", &[]),
        Some(1)
    );
    models::call(
        "fixture",
        "private-unregistered-provider",
        CallOutcome::Provider,
        Duration::ZERO,
        None,
    );
    models::catalogue(None);
    assert_eq!(
        captured.gauge("agentsfleet_model_catalog_snapshot_age_seconds", &[]),
        None
    );
}

fn assert_token_observations(captured: &Capture) {
    models::tokens(
        "gpt-fixture",
        "openai",
        afd_wire::usage::ObservedTokens {
            input: Some(10),
            output: Some(3),
            cache_read: Some(4),
            cache_write: Some(2),
        },
    );
    models::tokens(
        "gpt-fixture",
        "openai",
        afd_wire::usage::ObservedTokens::default(),
    );
    let base = [
        ("gen_ai.request.model", "gpt-fixture"),
        ("gen_ai.provider.name", "openai"),
    ];
    let [model_label, provider_label] = base;
    let input = [model_label, provider_label, ("gen_ai.token.type", "input")];
    let output = [model_label, provider_label, ("gen_ai.token.type", "output")];
    for (family, labels, expected) in [
        ("agentsfleet_model_tokens_total", input.as_slice(), 10),
        ("agentsfleet_model_tokens_total", output.as_slice(), 3),
        (
            "agentsfleet_model_cache_read_tokens_total",
            base.as_slice(),
            4,
        ),
        (
            "agentsfleet_model_cache_write_tokens_total",
            base.as_slice(),
            2,
        ),
    ] {
        assert_eq!(
            captured.sum(family, labels),
            expected,
            "{family} {labels:?}"
        );
    }
}
