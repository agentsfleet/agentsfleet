//! sandbox_bind_guard.zig — the half of the bind check only the runner can do.
//!
//! `protocol.extraBindsValid` refuses every protected path by NAME, and both
//! sides run it. It is lexical by necessity: the control plane validates an
//! assignment from a machine where the operator's paths do not exist, so a
//! string is all it has to judge.
//!
//! bubblewrap resolves symlinks itself when it opens a bind source. So a
//! declared `/srv/shared` that links to `/etc` satisfies every string rule and
//! still mounts the host's `/etc` into the lease — writable, on every lease that
//! runner takes, if the assignment said `read_write`. Closing that needs a
//! filesystem, which only the runner has. Split from `sandbox_args` (RULE FLL)
//! because it is a different question: that file composes argv, this one decides
//! whether an argv may be composed at all.

const std = @import("std");
const contract = @import("contract");
const logging = @import("log");

const log = logging.scoped(.runner_sandbox_bind_guard);

/// Refuse the lease when an operator bind RESOLVES onto a path the sandbox
/// protects, however it was spelled.
///
/// Absent is not unsafe. Every bind is emitted with a `-try` flag precisely so a
/// path this host lacks is skipped rather than failing the lease, so a lookup
/// that finds nothing leaves the entry alone — it will mount nothing. Any OTHER
/// resolution failure fails the lease: a path we cannot resolve is a path we
/// cannot prove safe, and this is the wrong place to guess.
///
/// Fails the WHOLE list, matching `extraBindsValid` — a partially applied bind
/// set is a sandbox nobody reasoned about.
///
/// Residual, and deliberately not claimed as closed: this check and
/// bubblewrap's own open are separate syscalls, so a link swapped in between
/// still redirects the mount. Closing it needs the resolved path to become the
/// bind SOURCE while the declared path stays the destination, which changes what
/// `composeBinds` carries. That window is far narrower than the one this
/// removes, and reaching it already requires host write access.
pub fn assertBindTargetsSafe(io: std.Io, extra_binds: []const contract.protocol.ExtraBind) !void {
    var buf: [contract.protocol.MAX_BIND_PATH_LEN]u8 = undefined;
    for (extra_binds) |b| {
        const n = std.Io.Dir.realPathFileAbsolute(io, b.path, &buf) catch |err| switch (err) {
            error.FileNotFound, error.NotDir => continue,
            else => {
                log.warn("sandbox_bind_unresolvable", .{ .path = b.path, .err = @errorName(err) });
                return error.UnsafeBindTarget;
            },
        };
        if (contract.protocol.pathOverlapsProtected(buf[0..n])) {
            // The declared path is logged beside the resolved one: an operator
            // reading only the target would not recognise what they assigned.
            log.warn("sandbox_bind_resolves_onto_protected_path", .{
                .declared = b.path,
                .resolved = buf[0..n],
                .mode = b.mode.label(),
            });
            return error.UnsafeBindTarget;
        }
    }
}
