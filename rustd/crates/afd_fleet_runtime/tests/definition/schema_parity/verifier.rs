//! Every declared recipe has the same header rules in parser and schema.
use super::{Definition, components, definition_component};
use serde_json::{Value, json};

#[test]
fn the_verifier_schema_and_parser_agree() {
    let components = components();
    for family in afd_webhook::Family::ALL {
        let fixed = matches!(
            family,
            afd_webhook::Family::SlackV0 | afd_webhook::Family::SvixV1
        );
        for header in [
            None,
            Some(json!("x-proof")),
            Some(json!("X-Proof")),
            Some(json!("x".repeat(128))),
            Some(json!("x".repeat(129))),
            Some(json!("")),
            Some(json!("x proof")),
            Some(json!(null)),
            Some(json!(42)),
        ] {
            let mut declaration = json!({"scheme":family.token()});
            if let Some(value) = &header {
                declaration
                    .as_object_mut()
                    .unwrap_or_else(|| panic!("the declaration is an object"))
                    .insert("header".to_owned(), value.clone());
            }
            let accepted = accepts_header(fixed, header.as_ref());
            let mut document = super::super::value();
            super::super::set(
                &mut document,
                "/config/x-agentsfleet/triggers",
                json!([{"type":"webhook","source":"acme","verifier":declaration}]),
            );
            assert_eq!(
                Definition::parse(&document.to_string()).is_ok(),
                accepted,
                "{declaration}"
            );
            assert_eq!(
                components.admits(&definition_component(), &document),
                accepted,
                "{declaration}"
            );
        }
    }
}

fn accepts_header(fixed: bool, header: Option<&Value>) -> bool {
    if fixed {
        header.is_none()
    } else {
        header
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && s.len() <= 128 && !s.contains(' '))
    }
}
