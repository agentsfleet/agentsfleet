use std::error::Error as _;
use std::io;

use jsonrpsee_types::error::{
    CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE,
};

use super::{
    Error, connection_lost, input_backlog_full, input_closed, invalid_params, launch_incomplete,
    not_a_file, not_found, path_refused, refused, tenant_unavailable, unknown_process,
    unresponsive,
};
use crate::protocol::{
    FILE_NOT_FOUND_CODE, PATH_REFUSED_CODE, UNKNOWN_PROCESS_CODE, WIRE_MESSAGE_MAX_BYTES,
};

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
            tenant_unavailable(io::Error::from(io::ErrorKind::PermissionDenied)),
            INTERNAL_ERROR_CODE,
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
    // A workspace name that is not there is sorted by the file calls, not
    // here: a program that is not there is still only the caller's mistake.
    let missing = not_found(io::Error::from(io::ErrorKind::NotFound));
    assert_eq!(missing.rpc_code(), FILE_NOT_FOUND_CODE, "{missing}");
    assert!(
        missing
            .wire_message()
            .starts_with("the workspace has no such file or directory: "),
        "{}",
        missing.wire_message()
    );
}

/// A handler reads the refusal where it is raised, in the executor, or as the
/// answer the client decoded; each predicate holds on both and on nothing else.
#[test]
fn a_refused_path_and_a_missing_name_read_the_same_on_both_ends_of_the_socket() {
    let path_raised = path_refused();
    let path_decoded = refused(PATH_REFUSED_CODE, "outside");
    let missing_raised = not_found(io::Error::from(io::ErrorKind::NotFound));
    let missing_decoded = refused(FILE_NOT_FOUND_CODE, "absent");

    assert!(path_raised.is_path_refused() && path_decoded.is_path_refused());
    assert!(missing_raised.is_not_found() && missing_decoded.is_not_found());
    assert!(!path_raised.is_not_found() && !path_decoded.is_not_found());
    assert!(!missing_raised.is_path_refused() && !missing_decoded.is_path_refused());
    for other in [
        not_a_file(),
        unknown_process(),
        refused(INVALID_PARAMS_CODE, "bad"),
        Error::from(io::Error::from(io::ErrorKind::NotFound)),
        Error::from(io::Error::from(io::ErrorKind::PermissionDenied)),
    ] {
        assert!(!other.is_path_refused() && !other.is_not_found(), "{other}");
    }
}

/// A write's two process-level refusals read the same on both ends, and
/// nothing else reads as either.
#[test]
fn a_gone_process_and_a_refused_input_read_the_same_on_both_ends_of_the_socket() {
    let gone_raised = unknown_process();
    let gone_decoded = refused(UNKNOWN_PROCESS_CODE, "no process");
    let closed_raised = input_closed();
    let full_raised = input_backlog_full();
    let input_decoded = refused(CALL_EXECUTION_FAILED_CODE, "not reading");

    assert!(gone_raised.is_unknown_process() && gone_decoded.is_unknown_process());
    assert!(closed_raised.is_input_refused() && full_raised.is_input_refused());
    assert!(input_decoded.is_input_refused());
    assert!(!gone_raised.is_input_refused() && !gone_decoded.is_input_refused());
    assert!(!closed_raised.is_unknown_process() && !input_decoded.is_unknown_process());
    for other in [
        path_refused(),
        not_found(io::Error::from(io::ErrorKind::NotFound)),
        connection_lost(),
        refused(PATH_REFUSED_CODE, "outside"),
        Error::from(io::Error::from(io::ErrorKind::BrokenPipe)),
    ] {
        assert!(
            !other.is_unknown_process() && !other.is_input_refused(),
            "{other}"
        );
    }
}

#[test]
fn a_tenant_leaf_that_cannot_be_entered_keeps_the_kernels_reason_as_its_cause() {
    let failure = tenant_unavailable(io::Error::other("descriptor lost"));

    assert_eq!(
        failure.source().map(ToString::to_string).as_deref(),
        Some("descriptor lost")
    );
    assert_eq!(
        failure.wire_message(),
        "the tenant leaf cannot be entered: descriptor lost"
    );
}

/// `wire_message` renders at most the cap, whole when it fits, cut on a
/// character boundary otherwise, from whatever the socket sent: a refusal's
/// own message, or a value a result could not decode, which serde echoes.
#[test]
fn a_wire_message_is_kept_to_the_cap_whatever_carried_it() {
    let prefix = format!("the executor refused the call ({INTERNAL_ERROR_CODE}): ");
    let room = WIRE_MESSAGE_MAX_BYTES - prefix.len();
    let at_cap = "x".repeat(room);
    let over = "x".repeat(room + 1);
    // The cap falls one byte into a three-byte character, whatever its value.
    let straddle = format!("{}€", "a".repeat(room - 1));
    let flood = format!("\"{}\"", "x".repeat(2 * WIRE_MESSAGE_MAX_BYTES));
    let echoed =
        serde_json::from_str::<bool>(&flood).map_or_else(Error::from, |_| unknown_process());

    assert_eq!(
        refused(INTERNAL_ERROR_CODE, &at_cap).wire_message(),
        format!("{prefix}{at_cap}")
    );
    assert_eq!(
        refused(INTERNAL_ERROR_CODE, &over).wire_message(),
        format!("{prefix}{at_cap}")
    );
    assert_eq!(
        refused(INTERNAL_ERROR_CODE, &straddle).wire_message(),
        format!("{prefix}{}", "a".repeat(room - 1))
    );
    let said = echoed.wire_message();
    assert!(
        said.starts_with("a message did not decode: invalid type: string"),
        "{said}"
    );
    assert_eq!(said.len(), WIRE_MESSAGE_MAX_BYTES);
}

/// The stored message of a refusal is cut as it is raised, apart from the
/// cut at the wire: a log line rendering the error is bounded too.
#[test]
fn a_refusals_stored_message_is_cut_as_it_is_raised() {
    let prefix = format!("the executor refused the call ({INTERNAL_ERROR_CODE}): ");
    let flood = "x".repeat(2 * WIRE_MESSAGE_MAX_BYTES);

    let logged = refused(INTERNAL_ERROR_CODE, &flood).to_string();

    let first_line = logged.lines().next().unwrap_or_default();
    assert_eq!(
        first_line.len(),
        "[UZ-INTERNAL-003] ".len() + prefix.len() + WIRE_MESSAGE_MAX_BYTES,
        "{first_line:.80}"
    );
}
