use std::error::Error as _;

use super::{connection_lost, invalid_params, path_refused, refused, unknown_process};

#[test]
fn every_failure_without_a_cause_renders_and_reports_none() {
    for (failure, variant) in [
        (connection_lost(), "ConnectionLost"),
        (refused(-32_603, "the executor fell over"), "Refused"),
        (path_refused(), "PathRefused"),
        (unknown_process(), "UnknownProcess"),
        (invalid_params("argv must name a program"), "InvalidParams"),
    ] {
        let rendered = failure.to_string();

        assert!(format!("{failure:?}").contains(variant), "{failure:?}");
        assert!(failure.source().is_none(), "{rendered}");
        // The wire carries the sentence alone: no registry code, no backtrace.
        assert_eq!(
            Some(failure.wire_message().as_str()),
            rendered
                .strip_prefix("[UZ-INTERNAL-003] ")
                .and_then(|rest| rest.lines().next()),
        );
    }
}
