//! What a sandbox failure says about itself, apart from its code.

use super::cgroup_unreadable;

/// The control file the failure names.
const FILE: &str = "cgroup.events";

#[test]
fn test_detail_is_the_failures_sentence_without_its_code() {
    let failure = cgroup_unreadable(FILE)(std::io::Error::other("gone"));

    let shown = failure.to_string();
    let detail = failure.detail();

    assert_eq!(detail, format!("the cgroup file {FILE} could not be read"));
    assert!(
        shown.starts_with('[') && shown.contains(&detail),
        "the code leads the shown failure and the detail follows: {shown}"
    );
}

/// A log's reason is the failure's sentence, then each cause beneath it, and
/// never the code the log's `error_code` field already carries.
#[test]
fn test_told_is_the_sentence_then_each_cause_without_the_code() {
    let failure = cgroup_unreadable(FILE)(std::io::Error::other("gone"));

    let told = failure.told();

    assert_eq!(told, format!("{}: gone", failure.detail()));
    assert!(!told.contains(failure.code().as_str()), "{told}");
}
