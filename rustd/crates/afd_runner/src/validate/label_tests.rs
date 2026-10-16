//! Enrolment and the operator's runner edit share one label rule.
//!
//! Each case runs the same list through both writers and asserts they agree,
//! so a rule that moved on one side alone fails here by name.

#![expect(
    clippy::expect_used,
    reason = "a proved label set is a fixture precondition"
)]

use afd_wire::runner::{NetworkPolicy, SandboxTier};

use super::*;

fn owned(labels: &[&str]) -> Vec<Cow<'static, str>> {
    labels
        .iter()
        .map(|label| Cow::Owned((*label).to_owned()))
        .collect()
}

fn enrolment(labels: Vec<Cow<'static, str>>) -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("runner.example"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::DevNone,
            network_policy: NetworkPolicy::AllowAll,
            registry_allowlist: Vec::new(),
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels,
    }
}

/// What each writer stores for `given`, or the sentence each refuses it with.
fn both(given: &[Cow<'static, str>]) -> (Result<Vec<String>>, Result<Vec<String>>) {
    let stored = |labels: Vec<Cow<'_, str>>| labels.into_iter().map(Cow::into_owned).collect();
    let enrolled =
        registration(&enrolment(given.to_vec())).map(|stored_row| stored(stored_row.labels));
    let replaced = labels(given).map(stored);
    (enrolled, replaced)
}

fn refusal(result: &Result<Vec<String>>) -> Option<&'static str> {
    result.as_ref().err().map(crate::Error::detail)
}

#[test]
fn a_repeated_label_collapses_to_its_first_position_on_both_writers() {
    let given = owned(&["gpu", "linux", "gpu", "arm64", "linux"]);
    let (enrolled, replaced) = both(&given);
    let expected = vec!["gpu".to_owned(), "linux".to_owned(), "arm64".to_owned()];

    assert_eq!(enrolled.expect("enrolment accepts repeats"), expected);
    assert_eq!(replaced.expect("replacement accepts repeats"), expected);
}

#[test]
fn labels_differing_only_in_case_are_distinct() {
    // Byte equality, not a folded one: `required_tags <@ labels` compares
    // bytes, so folding here would place work a host cannot match.
    let (enrolled, replaced) = both(&owned(&["GPU", "gpu"]));
    let expected = vec!["GPU".to_owned(), "gpu".to_owned()];
    assert_eq!(enrolled.expect("case is significant"), expected);
    assert_eq!(replaced.expect("case is significant"), expected);
}

#[test]
fn too_many_labels_are_refused_with_the_enrolment_sentence() {
    let at_cap = vec![Cow::Borrowed("gpu"); LABELS_MAX];
    let (enrolled, replaced) = both(&at_cap);
    assert!(
        enrolled.is_ok() && replaced.is_ok(),
        "the cap itself is legal"
    );

    let distinct_at_cap: Vec<Cow<'static, str>> = (0..LABELS_MAX)
        .map(|index| Cow::Owned(format!("l{index}")))
        .collect();
    let (enrolled, replaced) = both(&distinct_at_cap);
    assert_eq!(enrolled.expect("cap of distinct labels").len(), LABELS_MAX);
    assert_eq!(replaced.expect("cap of distinct labels").len(), LABELS_MAX);

    // The count is proved before the collapse, so 33 copies of one label are
    // refused even though they would store as one.
    let past_cap = vec![Cow::Borrowed("gpu"); LABELS_MAX + 1];
    let (enrolled, replaced) = both(&past_cap);
    assert_eq!(refusal(&enrolled), Some(DETAIL_LABELS));
    assert_eq!(refusal(&replaced), Some(DETAIL_LABELS));
}

#[test]
fn an_over_long_label_is_refused_with_the_enrolment_sentence() {
    let at_edge = vec![Cow::Owned("l".repeat(LABEL_MAX_BYTES))];
    let (enrolled, replaced) = both(&at_edge);
    assert!(
        enrolled.is_ok() && replaced.is_ok(),
        "the edge itself is legal"
    );

    // Bytes, not characters: 22 three-byte characters is 66 bytes.
    for past_edge in ["l".repeat(LABEL_MAX_BYTES + 1), "€".repeat(22)] {
        let (enrolled, replaced) = both(&[Cow::Owned(past_edge)]);
        assert_eq!(refusal(&enrolled), Some(DETAIL_LABELS));
        assert_eq!(refusal(&replaced), Some(DETAIL_LABELS));
    }
}

#[test]
fn an_empty_label_is_treated_alike_by_both_writers() {
    // Enrolment has never refused an empty label; the replacement inherits
    // that rather than inventing a stricter rule of its own.
    let (enrolled, replaced) = both(&owned(&["", "gpu", ""]));
    let expected = vec![String::new(), "gpu".to_owned()];
    assert_eq!(enrolled.expect("enrolment's rule"), expected);
    assert_eq!(replaced.expect("the same rule"), expected);
}

#[test]
fn an_empty_set_clears_every_label() {
    let (enrolled, replaced) = both(&[]);
    assert_eq!(enrolled.expect("no labels is legal"), Vec::<String>::new());
    assert_eq!(replaced.expect("no labels is legal"), Vec::<String>::new());
}
