//! Where a tool runs, what a handler is lent for one call, and why a call
//! failed.

use std::fmt;

use afr_executor::Executor;
use guffbolt::CallRef;
use guffbolt::tools::code;

use crate::run::Run;

/// Where a tool's handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// In the supervisor, beside the loop. Never touches the sandbox.
    Supervisor,
    /// Inside the run's sandbox, through the executor connection.
    Sandbox,
    /// At the model provider, sent as the provider's own hosted tool spec.
    Provider,
}

/// What a handler is given for one call.
///
/// The router builds it per call and hands the executor only to a sandbox-side
/// handler, so a supervisor-side one cannot reach the sandbox by construction.
/// A generic tool reaches that executor as its `guffbolt` environment, and
/// nothing else: no handler here runs a command on the runner's host.
/// The run's state is shared: a run's child loops call tools at the same
/// time as their parent, so each part a call changes sits behind its own lock
/// ([`Run`]).
#[derive(Debug)]
pub struct ToolContext<'call, 'run> {
    /// The run's executor; `None` for a supervisor-side call.
    pub executor: Option<&'call dyn Executor>,
    /// What every call of the run shares.
    pub run: &'call Run<'run>,
    /// Where the call sits in its loop's conversation.
    pub call_ref: &'call CallRef,
}

/// One handler the catalog hosts: a `guffbolt` tool lent one run's
/// [`ToolContext`] per call, so the same handler serves every run and every
/// call. Its runtime is its published entry's, never its own claim.
pub type CatalogTool = dyn for<'call, 'run> guffbolt::Tool<ToolContext<'call, 'run>>;

/// A tool one run built over its own backends, lent the run's
/// [`ToolContext`] per call like a hosted handler, and living as long as the
/// backends it borrowed (`'b`).
pub type RunTool<'b> = dyn for<'call, 'run> guffbolt::Tool<ToolContext<'call, 'run>> + 'b;

/// Why a call failed, in the stable spelling the model and the thread read.
///
/// What `guffbolt` refuses itself, a call cut at the output limit and a
/// child refused or failed, carries `guffbolt::ErrorCode`'s own codes.
///
/// A fieldless vocabulary, kept a plain enum the way `afd_auth::Error` is
/// (`docs/RUST_ERROR_STANDARD.md` §"The shared hull"): it rides every refused
/// call's output, carries no cause, and is never boxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorCode {
    /// The model called a tool this run was not offered.
    NotOffered,
    /// The tool is the provider's to run, and this provider offers none.
    HostedToolUnavailable,
    /// A sandbox-side tool was called on a run that has no sandbox.
    SandboxUnavailable,
    /// The call's arguments do not parse as the tool's schema, or break a
    /// bound it declares.
    InvalidArguments,
    /// The memory this run stored would no longer fit one push.
    MemoryFull,
    /// The URL is not HTTPS.
    HttpsRequired,
    /// The method is outside what this tool or this policy sends.
    MethodNotAllowed,
    /// The host is not in the fleet's network allowlist.
    HostNotAllowed,
    /// The host is, or resolves to, a private, loopback or reserved address,
    /// which no request leaves for.
    EgressDenied,
    /// A placeholder, or a header, stands where none may.
    CredentialPlacementNotAllowed,
    /// The credential a placeholder names is not sent to this host.
    CredentialHostNotAllowed,
    /// A placeholder names a secret the fleet does not have.
    SecretNotFound,
    /// The host's origin rules admit no request of this shape.
    RequestPolicyNotAllowed,
    /// The daemon would not mint the credential a placeholder names.
    CredentialMintRefused,
    /// The request left and no answer came back.
    UpstreamUnreachable,
    /// The upstream answered with a status outside 2xx.
    UpstreamStatus,
    /// A memory was stored as shared, and this fleet may not publish.
    SharedMemoryNotGranted,
    /// The command ran past its timeout, and its process group was killed.
    TimedOut,
    /// The kernel killed the command's process because its sandbox's tenant
    /// processes ran out of memory.
    OutOfMemory,
    /// The process's ending never reached the caller: its sandbox or the
    /// executor went away.
    Interrupted,
    /// The session named is not open: it never was, or its process ended.
    SessionNotFound,
    /// The subcommand reaches a remote, which only the runner reaches.
    SubcommandNotAllowed,
    /// The path leaves the workspace, by name or through a link.
    PathNotAllowed,
    /// The path names no file or directory in the workspace.
    FileNotFound,
    /// The file is longer than the call carries: one read, for an edit; the
    /// image cap, for an image.
    FileTooLarge,
    /// The text to replace is not in the file.
    TextNotFound,
    /// A line tag no longer matches the file, or matches it more than once.
    HashMismatch,
    /// The patch does not parse, or a hunk's lines are not in the file.
    PatchInvalid,
    /// The file is not an image the wires take.
    NotAnImage,
    /// The model's wire takes no image with a call's result.
    ImageInputUnavailable,
    /// The browser tools wait for the Firecracker engine; this sandbox cannot
    /// start Chromium.
    BrowserUnavailable,
    /// The fleet already holds as many schedules as it may create itself.
    ScheduleCapReached,
    /// The schedule is a person's, so the fleet may read it and not change it.
    ScheduleNotFleetOwned,
    /// The schedule is paused or being removed, or a schedule started this
    /// run, so it does not run now.
    ScheduleNotRunnable,
    /// The event came from no thread, so a message has nowhere to go.
    MessageNoChannel,
    /// The run already posted as many messages as one run may.
    MessageLimitReached,
    /// `agentsfleetd` refused the call, for the reason its code names.
    AgentsfleetdRefused,
    /// `agentsfleetd` could not be reached.
    AgentsfleetdUnreachable,
}

impl ToolErrorCode {
    /// The spelling the output text carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotOffered => guffbolt::ErrorCode::NOT_OFFERED.as_str(),
            Self::HostedToolUnavailable => "hosted_tool_unavailable",
            Self::SandboxUnavailable => code::SANDBOX_UNAVAILABLE.as_str(),
            Self::InvalidArguments => code::INVALID_ARGUMENTS.as_str(),
            Self::MemoryFull => code::MEMORY_FULL.as_str(),
            Self::HttpsRequired => code::HTTPS_REQUIRED.as_str(),
            Self::MethodNotAllowed => code::METHOD_NOT_ALLOWED.as_str(),
            Self::HostNotAllowed => code::HOST_NOT_ALLOWED.as_str(),
            Self::EgressDenied => code::EGRESS_DENIED.as_str(),
            Self::CredentialPlacementNotAllowed => "credential_placement_not_allowed",
            Self::CredentialHostNotAllowed => "credential_host_not_allowed",
            Self::SecretNotFound => code::SECRET_NOT_FOUND.as_str(),
            Self::RequestPolicyNotAllowed => "request_policy_not_allowed",
            Self::CredentialMintRefused => "credential_mint_refused",
            Self::UpstreamUnreachable => code::UPSTREAM_UNREACHABLE.as_str(),
            Self::UpstreamStatus => code::UPSTREAM_STATUS.as_str(),
            Self::SharedMemoryNotGranted => code::SHARED_MEMORY_NOT_GRANTED.as_str(),
            Self::TimedOut => code::TIMED_OUT.as_str(),
            Self::OutOfMemory => code::OUT_OF_MEMORY.as_str(),
            Self::Interrupted => guffbolt::ErrorCode::INTERRUPTED.as_str(),
            Self::SessionNotFound => code::SESSION_NOT_FOUND.as_str(),
            Self::SubcommandNotAllowed => code::SUBCOMMAND_NOT_ALLOWED.as_str(),
            Self::PathNotAllowed => code::PATH_NOT_ALLOWED.as_str(),
            Self::FileNotFound => code::FILE_NOT_FOUND.as_str(),
            Self::FileTooLarge => code::FILE_TOO_LARGE.as_str(),
            Self::TextNotFound => code::TEXT_NOT_FOUND.as_str(),
            Self::HashMismatch => code::HASH_MISMATCH.as_str(),
            Self::PatchInvalid => code::PATCH_INVALID.as_str(),
            Self::NotAnImage => code::NOT_AN_IMAGE.as_str(),
            Self::ImageInputUnavailable => code::IMAGE_INPUT_UNAVAILABLE.as_str(),
            Self::BrowserUnavailable => code::BROWSER_UNAVAILABLE.as_str(),
            Self::ScheduleCapReached => "schedule_cap_reached",
            Self::ScheduleNotFleetOwned => "schedule_not_fleet_owned",
            Self::ScheduleNotRunnable => "schedule_not_runnable",
            Self::MessageNoChannel => "message_no_channel",
            Self::MessageLimitReached => "message_limit_reached",
            Self::AgentsfleetdRefused => "agentsfleetd_refused",
            Self::AgentsfleetdUnreachable => "agentsfleetd_unreachable",
        }
    }
}

impl fmt::Display for ToolErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The code a call's output carries, spelled as this vocabulary spells it.
impl From<ToolErrorCode> for guffbolt::ErrorCode {
    fn from(code: ToolErrorCode) -> Self {
        Self::new(code.as_str())
    }
}

#[cfg(test)]
mod tests;
