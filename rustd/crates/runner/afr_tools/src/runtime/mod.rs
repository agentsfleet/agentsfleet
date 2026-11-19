//! Where a tool runs, what a handler is lent for one call, and why a call
//! failed.

use std::fmt;

use guffbolt::ToolOutput;
use serde::de::DeserializeOwned;

use afr_executor::Executor;

use crate::lease::Lease;

/// Where a tool's handler runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    /// In the supervisor, beside the loop. Never touches the sandbox.
    Supervisor,
    /// Inside the lease's sandbox, through the executor connection.
    Sandbox,
    /// At the model provider, sent as the provider's own hosted tool spec.
    Provider,
}

/// What a handler is given for one call.
///
/// The router builds it per call and hands the executor only to a sandbox-side
/// handler, so a supervisor-side one cannot reach the sandbox by construction.
/// The lease's state is shared: a run's child loops call tools at the same
/// time as their parent, so each part a call changes sits behind its own lock
/// ([`Lease`]).
#[derive(Debug)]
pub struct ToolContext<'call, 'run> {
    /// The lease's executor; `None` for a supervisor-side call.
    pub executor: Option<&'call dyn Executor>,
    /// What every call of the lease shares.
    pub lease: &'call Lease<'run>,
}

/// One handler the catalog hosts: a `guffbolt` tool lent one lease's
/// [`ToolContext`] per call, so the same handler serves every lease and every
/// call. Its runtime is its published entry's, never its own claim.
pub type CatalogTool = dyn for<'call, 'run> guffbolt::Tool<ToolContext<'call, 'run>>;

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
    /// The host is, or resolves to, a private, loopback or reserved address.
    AddressNotAllowed,
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
    /// A store asked the workspace to read it, and this fleet may not publish.
    WorkspaceMemoryNotGranted,
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
            Self::SandboxUnavailable => "sandbox_unavailable",
            Self::InvalidArguments => "invalid_arguments",
            Self::MemoryFull => "memory_full",
            Self::HttpsRequired => "https_required",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::HostNotAllowed => "host_not_allowed",
            Self::AddressNotAllowed => "address_not_allowed",
            Self::CredentialPlacementNotAllowed => "credential_placement_not_allowed",
            Self::CredentialHostNotAllowed => "credential_host_not_allowed",
            Self::SecretNotFound => "secret_not_found",
            Self::RequestPolicyNotAllowed => "request_policy_not_allowed",
            Self::CredentialMintRefused => "credential_mint_refused",
            Self::UpstreamUnreachable => "upstream_unreachable",
            Self::UpstreamStatus => "upstream_status",
            Self::WorkspaceMemoryNotGranted => "workspace_memory_not_granted",
            Self::TimedOut => "timed_out",
            Self::OutOfMemory => "out_of_memory",
            Self::Interrupted => guffbolt::ErrorCode::INTERRUPTED.as_str(),
            Self::SessionNotFound => "session_not_found",
            Self::SubcommandNotAllowed => "subcommand_not_allowed",
            Self::PathNotAllowed => "path_not_allowed",
            Self::FileNotFound => "file_not_found",
            Self::FileTooLarge => "file_too_large",
            Self::TextNotFound => "text_not_found",
            Self::HashMismatch => "hash_mismatch",
            Self::PatchInvalid => "patch_invalid",
            Self::NotAnImage => "not_an_image",
            Self::ImageInputUnavailable => "image_input_unavailable",
            Self::BrowserUnavailable => "browser_unavailable",
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

/// `arguments` as `A`, or the refusal the model reads when they do not parse
/// as `A`'s schema. The one place a call's arguments are parsed, whether a
/// handler runs them or the loop does.
///
/// # Errors
/// The arguments break the schema: a field missing, mistyped, or not named.
pub fn parsed<A: DeserializeOwned>(arguments: &serde_json::Value) -> Result<A, ToolOutput> {
    match A::deserialize(arguments) {
        Ok(arguments) => Ok(arguments),
        // The refusal carries serde's sentence to the model; no error chain
        // leaves here, since a parse the model got wrong is the model's to read.
        Err(refused) => Err(ToolOutput::failed(
            ToolErrorCode::InvalidArguments,
            &refused.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests;
