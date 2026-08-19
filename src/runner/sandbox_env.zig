//! sandbox_env.zig — the child-environment policy `child_process.forkExec`
//! applies at the spawn boundary.
//!
//! Split from `sandbox_args.zig` on the 350-line bound (RULE FLL): argv
//! composition and environ policy are enforced at different boundaries
//! (`buildArgv` vs `forkExec`), so the split follows the consumers.

/// Daemon env-var prefix that must NEVER reach a sandboxed child — the
/// control-plane credentials live here (incl. `AGENTSFLEET_RUNNER_TOKEN`). The
/// allowlist below already excludes it by omission; `forkExec` asserts it absent
/// from the child's environ regardless of allowlist contents (defense-in-depth).
pub const ENV_DENY_PREFIX = "AGENTSFLEET_";

/// The ONLY environment variables forwarded into a sandboxed child's environ
/// (RULE UFS — single source, referenced by `child_process.forkExec` + tests).
/// Fail-closed: the child inherits EXACTLY these (each only when the daemon has
/// it set) and nothing else, so the daemon environ never leaks. Derived from a
/// verified enumeration of every in-child env read (our `runner_observer`, the
/// NullClaw engine, and tool subprocesses). `RUNNER_*` (parent-only daemon
/// config) and `NULLCLAW_PROVIDER`/`NULLCLAW_MODEL` (delivered on the lease, not
/// env) are deliberately excluded.
pub const ENV_PASSTHROUGH_ALLOWLIST = [_][]const u8{
    "HOME", // NullClaw config dir; absence → error.HomeDirNotFound (load-bearing)
    "PATH", // tool subprocess + wasmtime executable resolution (load-bearing)
    "NULLCLAW_OBSERVER", // optional observer-backend selector (safe default: log)
    "SSL_CERT_FILE", // TLS CA bundle override — pass-through-if-set
    "SSL_CERT_DIR", // TLS CA directory override — pass-through-if-set
    "LANG", // locale — pass-through-if-set
    "LC_ALL", // locale — pass-through-if-set
};
