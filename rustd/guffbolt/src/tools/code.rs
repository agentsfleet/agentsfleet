//! Why a generic tool's call failed, in the spellings the model reads.
//!
//! One constant per code, so an embedder that keeps its own closed set of
//! codes converts from these and every spelling lives here once.

use crate::tool::ErrorCode;

/// The call's arguments do not parse as the tool's schema, or break a bound
/// it declares.
pub const INVALID_ARGUMENTS: ErrorCode = ErrorCode::new("invalid_arguments");
/// A tool that runs in the environment was called on a run that has none, or
/// the environment could not be reached or would not do it.
pub const SANDBOX_UNAVAILABLE: ErrorCode = ErrorCode::new("sandbox_unavailable");
/// The memory this run stored would no longer fit what its store keeps.
pub const MEMORY_FULL: ErrorCode = ErrorCode::new("memory_full");
/// A store asked the workspace to read it, and this run may not publish.
pub const WORKSPACE_MEMORY_NOT_GRANTED: ErrorCode = ErrorCode::new("workspace_memory_not_granted");
/// The URL is not HTTPS.
pub const HTTPS_REQUIRED: ErrorCode = ErrorCode::new("https_required");
/// The method is outside what this tool or this policy sends.
pub const METHOD_NOT_ALLOWED: ErrorCode = ErrorCode::new("method_not_allowed");
/// The host is not one the outbound policy allows.
pub const HOST_NOT_ALLOWED: ErrorCode = ErrorCode::new("host_not_allowed");
/// A secret the tool needs is not one the run holds.
pub const SECRET_NOT_FOUND: ErrorCode = ErrorCode::new("secret_not_found");
/// The request left and no answer came back.
pub const UPSTREAM_UNREACHABLE: ErrorCode = ErrorCode::new("upstream_unreachable");
/// The upstream answered with a status outside 2xx.
pub const UPSTREAM_STATUS: ErrorCode = ErrorCode::new("upstream_status");
/// The command ran past its timeout, and its process was killed.
pub const TIMED_OUT: ErrorCode = ErrorCode::new("timed_out");
/// The command's process was killed because its environment ran out of
/// memory.
pub const OUT_OF_MEMORY: ErrorCode = ErrorCode::new("out_of_memory");
/// The session named is not open: it never was, or its process ended.
pub const SESSION_NOT_FOUND: ErrorCode = ErrorCode::new("session_not_found");
/// The subcommand reaches a remote, which the environment does not.
pub const SUBCOMMAND_NOT_ALLOWED: ErrorCode = ErrorCode::new("subcommand_not_allowed");
/// The path leaves the workspace, by name or through a link.
pub const PATH_NOT_ALLOWED: ErrorCode = ErrorCode::new("path_not_allowed");
/// The path names no file or directory in the workspace.
pub const FILE_NOT_FOUND: ErrorCode = ErrorCode::new("file_not_found");
/// The file is longer than the call carries.
pub const FILE_TOO_LARGE: ErrorCode = ErrorCode::new("file_too_large");
/// The text to replace is not in the file.
pub const TEXT_NOT_FOUND: ErrorCode = ErrorCode::new("text_not_found");
/// A line tag no longer matches the file, or matches it more than once.
pub const HASH_MISMATCH: ErrorCode = ErrorCode::new("hash_mismatch");
/// The patch does not parse, or a hunk's lines are not in the file.
pub const PATCH_INVALID: ErrorCode = ErrorCode::new("patch_invalid");
/// The file is not an image the wires take.
pub const NOT_AN_IMAGE: ErrorCode = ErrorCode::new("not_an_image");
/// The model's wire takes no image with a call's result.
pub const IMAGE_INPUT_UNAVAILABLE: ErrorCode = ErrorCode::new("image_input_unavailable");
/// The environment has no browser to drive.
pub const BROWSER_UNAVAILABLE: ErrorCode = ErrorCode::new("browser_unavailable");
/// The MCP server would not answer, or answered with an error.
pub const MCP_FAILED: ErrorCode = ErrorCode::new("mcp_failed");
/// No MCP server of the run has the name the call gave.
pub const MCP_SERVER_NOT_FOUND: ErrorCode = ErrorCode::new("mcp_server_not_found");
