use super::Outcome;

#[test]
fn runner_outcomes_use_the_same_spelling_in_rows_and_json() {
    for (outcome, stored) in [
        (Outcome::Processed, "processed"),
        (Outcome::FleetError, "fleet_error"),
    ] {
        assert_eq!(outcome.as_str(), stored);
        let encoded = serde_json::to_value(outcome);
        assert_eq!(
            encoded.as_ref().ok().and_then(serde_json::Value::as_str),
            Some(stored)
        );
    }
}
