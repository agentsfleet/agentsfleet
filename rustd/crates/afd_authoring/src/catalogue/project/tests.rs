//! Labels come from the catalogue; eligibility follows the shared predicates.

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
fn authoring_refuses_missing_keys_custom_endpoints_and_unsupported_models() {
    let mut missing = row();
    missing.has_key = false;
    let mut custom = row();
    custom.base_url = Some("https://customer.example/v1".into());
    let mut unsupported = row();
    unsupported.model = "gpt-6.1-sol".into();
    for choice in [missing, custom, unsupported] {
        assert!(eligible(choice, Some(ModelPurpose::Authoring)).is_none());
    }
}

#[test]
fn an_invalid_runtime_route_is_not_offered_for_runtime_or_any_purpose() {
    for purpose in [None, Some(ModelPurpose::Runtime)] {
        let mut invalid = row();
        invalid.base_url = Some("http://127.0.0.1:11434".into());
        assert!(eligible(invalid, purpose).is_none());
    }
}
