//! Labels come from the catalogue; eligibility follows the shared predicates.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{Row, eligible};
use crate::types::ModelPurpose;

fn row() -> Row {
    Row {
        reference: "configured-reference".into(),
        provider: "openai".into(),
        model: "gpt-4.1".into(),
        display_name: "Provider supplied display name".into(),
        base_url: None,
        has_key: true,
        context: 128_000,
        input: 2_000_000_000,
        output: 8_000_000_000,
        inference: Some(sqlx::types::Json(afd_wire::inference::ModelInference {
            api: afd_wire::inference::InferenceApi::Responses,
            base_url: "https://api.openai.com/v1".into(),
            dialect: None,
            max_output_tokens: 8192,
        })),
    }
}

#[test]
fn a_configured_choice_keeps_its_catalogue_label_and_exact_identity() {
    for purpose in [
        None,
        Some(ModelPurpose::Authoring),
        Some(ModelPurpose::Runtime),
    ] {
        let choice = eligible(row(), purpose).expect("a priced supported route is eligible");
        assert_eq!(choice.label, "Provider supplied display name");
        assert_eq!(choice.model, "gpt-4.1");
        assert_eq!(choice.reference, "configured-reference");
        assert!(choice.eligibility.authoring && choice.eligibility.runtime);
        assert_eq!(choice.pricing.input_nanos_per_million, "2000000000");
    }
}

#[test]
fn authoring_refuses_missing_keys_and_endpoints_the_saved_route_does_not_name() {
    let mut missing = row();
    missing.has_key = false;
    let mut custom = row();
    custom.base_url = Some("https://customer.example/v1".into());
    for choice in [missing, custom] {
        assert!(eligible(choice, Some(ModelPurpose::Authoring)).is_none());
    }
}

// Routing reads saved settings, never the model identifier: a model name the
// catalogue has no family for still routes when its saved API carries tools.
#[test]
fn a_model_name_alone_never_decides_eligibility() {
    let mut unfamiliar = row();
    unfamiliar.model = "gpt-6.1-sol".into();
    assert!(eligible(unfamiliar, Some(ModelPurpose::Authoring)).is_some());
}

#[test]
fn an_invalid_runtime_route_is_refused_for_runtime_and_listed_as_ineligible() {
    let invalid = || {
        let mut invalid = row();
        invalid.base_url = Some("http://127.0.0.1:11434".into());
        invalid
    };
    assert!(eligible(invalid(), Some(ModelPurpose::Runtime)).is_none());
    let listed =
        eligible(invalid(), None).expect("an unfiltered listing shows every configured model");
    assert!(!listed.eligibility.runtime && !listed.eligibility.authoring);
}
