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
