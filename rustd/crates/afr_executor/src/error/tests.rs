use std::error::Error as _;
use std::io;

use jsonrpsee_types::error::{
    CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE,
};

use super::{
    Error, connection_lost, input_backlog_full, input_closed, invalid_params, launch_incomplete,
    not_a_file, path_refused, program_unavailable, refused, unknown_process, unresponsive,
};
use crate::protocol::{PATH_REFUSED_CODE, UNKNOWN_PROCESS_CODE};

#[test]
fn every_failure_without_a_cause_renders_and_reports_none() {
    for (failure, variant) in [
        (connection_lost(), "ConnectionLost"),
        (unresponsive("fs/read"), "Unresponsive"),
        (refused(-32_603, "the executor fell over"), "Refused"),
        (path_refused(), "PathRefused"),
        (not_a_file(), "NotAFile"),
        (unknown_process(), "UnknownProcess"),
        (input_backlog_full(), "InputBacklogFull"),
        (input_closed(), "InputClosed"),
        (launch_incomplete(), "LaunchIncomplete"),
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

#[test]
fn each_refusal_answers_with_the_code_that_says_whose_it_is() {
    for (failure, code) in [
        (path_refused(), PATH_REFUSED_CODE),
        (unknown_process(), UNKNOWN_PROCESS_CODE),
        (input_backlog_full(), CALL_EXECUTION_FAILED_CODE),
        (input_closed(), CALL_EXECUTION_FAILED_CODE),
        (launch_incomplete(), INTERNAL_ERROR_CODE),
        (not_a_file(), INVALID_PARAMS_CODE),
        (
            program_unavailable("not on the search path"),
            INVALID_PARAMS_CODE,
        ),
        (invalid_params("bad"), INVALID_PARAMS_CODE),
        (connection_lost(), INTERNAL_ERROR_CODE),
    ] {
        assert_eq!(failure.rpc_code(), code, "{failure}");
    }
}

#[test]
fn an_operating_system_refusal_is_the_callers_only_when_it_is_about_the_name() {
    let callers = [
        io::ErrorKind::NotFound,
        io::ErrorKind::NotADirectory,
        io::ErrorKind::IsADirectory,
        io::ErrorKind::AlreadyExists,
        io::ErrorKind::DirectoryNotEmpty,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::InvalidInput,
        io::ErrorKind::InvalidFilename,
    ];
    let executors = [
        io::ErrorKind::StorageFull,
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::Other,
    ];

    for kind in callers {
        assert_eq!(
            Error::from(io::Error::from(kind)).rpc_code(),
            INVALID_PARAMS_CODE,
            "{kind:?}"
        );
    }
    for kind in executors {
        assert_eq!(
            Error::from(io::Error::from(kind)).rpc_code(),
            INTERNAL_ERROR_CODE,
            "{kind:?}"
        );
    }
}

#[test]
fn a_program_that_will_not_start_keeps_the_launchers_reason_as_its_cause() {
    let failure = program_unavailable("not on the search path");

    assert_eq!(
        failure.source().map(ToString::to_string).as_deref(),
        Some("not on the search path")
    );
    assert_eq!(
        failure.wire_message(),
        "the program could not be started: not on the search path"
    );
}
