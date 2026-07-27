//! Count and length ceilings on a bundle's declared requirements, and on the
//! operator-written install-gate copy that accompanies them.
//!
//! ## Why this module exists
//!
//! `requirements` and `required_credentials_reasons` are the two fields §3's
//! amended Fleet summary KEEPS — both are rendered at the moment a user decides
//! whether to install, so neither could be shed to the detail route. Keeping
//! them on a paged card makes their size part of the page's size, and neither
//! had an enforced bound: the importer capped `source_ref` and the support files
//! but never the requirement LISTS, and the curate path validated that
//! `required_credentials_reasons` was an object of strings without bounding how
//! many entries it held or how long each one ran. Small in practice, unbounded
//! in principle — and "in principle" is what a page-size ceiling is measured
//! against.
//!
//! ## Why the caps live here rather than at each door
//!
//! Two doors write these fields — the importer (from a bundle's `TRIGGER.md`)
//! and the admin curate patch (operator copy) — and they share exactly one rule
//! about what a credential NAME may be. Spelling it at both is how the same
//! field ends up with two different rules; `types/model_identity.zig` is the
//! precedent this follows, for the same reason.
//!
//! ## What the numbers buy
//!
//! The counts and lengths multiply out to a hard ceiling on the encoded
//! `requirements` blob of roughly 35 KB — `MAX_REQUIRED_CREDENTIALS` and
//! `MAX_REQUIRED_TOOLS` at `MAX_REQUIREMENT_NAME_LEN`, plus
//! `MAX_NETWORK_HOSTS` at `MAX_NETWORK_HOST_LEN`. That is what makes §3's
//! per-item projection bounded by construction rather than by observation, so
//! `UZ-LIBRARY-005` stays the unreachable invariant breach §Error Contracts
//! says it is instead of something a large enough bundle can provoke.

/// One credential row in the install gate is one thing a user has to go and
/// connect. Past this the gate is not a form, it is a migration project.
pub const MAX_REQUIRED_CREDENTIALS: usize = 32;

/// Tools are declared, not connected, so the ceiling is looser than credentials.
pub const MAX_REQUIRED_TOOLS: usize = 64;

/// Egress allow-list entries. Matches the tool ceiling: both are lists a bundle
/// author writes by hand, and neither has a per-entry cost at install time.
pub const MAX_NETWORK_HOSTS: usize = 64;

/// A credential or tool name. 200 is not a new policy — it is the display-copy
/// cap `catalog_patch.zig` already applies to an entry's `name`, applied to the
/// other names on the same resource.
pub const MAX_REQUIREMENT_NAME_LEN: usize = 200;

/// The maximum length of a fully-qualified domain name, as the Domain Name
/// System (DNS) specification fixes it. A real bound rather than an invented
/// one: a longer string cannot resolve, so accepting it would only let an
/// unusable allow-list entry occupy page bytes.
pub const MAX_NETWORK_HOST_LEN: usize = 253;

/// One reason per credential is the most that can ever be rendered — the gate
/// shows copy for credentials the bundle declares, and the refetch prune drops
/// the rest. Equal to the credential ceiling on purpose.
pub const MAX_REASON_ENTRIES: usize = MAX_REQUIRED_CREDENTIALS;

/// "Why this fleet needs it" is a sentence or two next to a connect button.
pub const MAX_REASON_LEN: usize = 500;

/// Which ceiling a value crossed. Distinct variants so a test can assert the
/// cap that fired rather than that something, somewhere, was too large; both
/// write paths collapse them onto their own existing over-size response.
pub const LimitError = error{
    TooManyCredentials,
    TooManyTools,
    TooManyNetworkHosts,
    RequirementNameTooLong,
    NetworkHostTooLong,
    TooManyReasons,
    ReasonTooLong,
};

/// Every requirement list a bundle declares, checked for count and per-entry
/// length. Callers pass the parsed `TRIGGER.md` lists verbatim.
pub fn validateRequirements(
    credentials: []const []const u8,
    tools: []const []const u8,
    network_hosts: []const []const u8,
) LimitError!void {
    if (credentials.len > MAX_REQUIRED_CREDENTIALS) return LimitError.TooManyCredentials;
    if (tools.len > MAX_REQUIRED_TOOLS) return LimitError.TooManyTools;
    if (network_hosts.len > MAX_NETWORK_HOSTS) return LimitError.TooManyNetworkHosts;
    for (credentials) |name| {
        if (name.len > MAX_REQUIREMENT_NAME_LEN) return LimitError.RequirementNameTooLong;
    }
    for (tools) |name| {
        if (name.len > MAX_REQUIREMENT_NAME_LEN) return LimitError.RequirementNameTooLong;
    }
    for (network_hosts) |host| {
        if (host.len > MAX_NETWORK_HOST_LEN) return LimitError.NetworkHostTooLong;
    }
}

/// One `required_credentials_reasons` entry: the key is a credential name, so it
/// takes the same bound as one in `required_credentials`.
pub fn validateReason(name: []const u8, reason: []const u8) LimitError!void {
    if (name.len > MAX_REQUIREMENT_NAME_LEN) return LimitError.RequirementNameTooLong;
    if (reason.len > MAX_REASON_LEN) return LimitError.ReasonTooLong;
}

/// How many entries the reasons map may hold.
pub fn validateReasonCount(count: usize) LimitError!void {
    if (count > MAX_REASON_ENTRIES) return LimitError.TooManyReasons;
}

test {
    _ = @import("requirement_limits_test.zig");
}
