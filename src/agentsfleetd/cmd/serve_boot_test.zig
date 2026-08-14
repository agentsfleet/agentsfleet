//! Wiring proofs for the boot-stage middleware registry.
//!
//! Most of `serve_boot` ends in `std.process.exit` on the failure arm, which a
//! unit test cannot survive; those refusals are proven by the boot smoke lanes.
//! `buildRegistry` is the exception and the one worth pinning: it decides which
//! lookup answers for which credential class, and a swap between two of them is
//! invisible at compile time — every field takes a function pointer of the same
//! shape, so `cli_credential_mw` wired to the tenant-key lookup would build,
//! boot, and authenticate the wrong principal class.

const std = @import("std");
const testing = std.testing;

const serve_boot = @import("serve_boot.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const cli_credential_lookup = @import("cli_credential_lookup.zig");
const clerk_scope_resolver = @import("../auth/clerk_scope_resolver.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");
const oidc_auth = @import("../auth/oidc.zig");

const APPROVAL_SECRET = "approval-signing-secret";

/// The registry stores its hosts type-erased, which is precisely why a swapped
/// pair compiles — the comparison has to erase too.
fn erase(ptr: anytype) *anyopaque {
    return @ptrCast(ptr);
}

/// Storage the registry only ever holds pointers into — `buildRegistry` copies
/// addresses and never dereferences, so the pointees stay uninitialised.
const Deps = struct {
    verifier: oidc_auth.Verifier,
    api_key_ctx: api_key_lookup.Ctx,
    cli_credential_ctx: cli_credential_lookup.Ctx,
    resolver: clerk_scope_resolver.ScopeResolver,
    runner_ctx: serve_runner_lookup.Ctx,

    fn init() Deps {
        // SAFETY: see above — no field is read by the function under test.
        return undefined;
    }

    fn asRegistryDeps(self: *Deps) serve_boot.RegistryDeps {
        return .{
            .verifier = &self.verifier,
            .api_key_lookup_ctx = &self.api_key_ctx,
            .cli_credential_lookup_ctx = &self.cli_credential_ctx,
            .scope_resolver = &self.resolver,
            .runner_lookup_ctx = &self.runner_ctx,
            .approval_signing_secret = APPROVAL_SECRET,
        };
    }
};

test "should wire each credential class to its own lookup" {
    var deps = Deps.init();

    const registry = serve_boot.buildRegistry(deps.asRegistryDeps());

    // A tenant API key and a CLI credential resolve different principals, so
    // crossing these two hosts would authenticate a person as a tenant-wide key
    // or vice versa — and both compile.
    try testing.expect(registry.tenant_api_key_mw.host == erase(&deps.api_key_ctx));
    try testing.expect(registry.cli_credential_mw.host == erase(&deps.cli_credential_ctx));
    try testing.expect(registry.runner_bearer_mw.host == erase(&deps.runner_ctx));
    try testing.expect(registry.bearer_or_api_key.verifier == &deps.verifier);
}

test "should share one scope resolver across both credential classes" {
    var deps = Deps.init();

    const registry = serve_boot.buildRegistry(deps.asRegistryDeps());

    // One instance, not two: the resolver owns the resolved-capability cache,
    // so a second instance would double every provider round trip for the same
    // subject and let the two classes disagree about a subject's scopes for as
    // long as the caches diverge.
    try testing.expect(registry.tenant_api_key_mw.scope_host == registry.cli_credential_mw.scope_host);
    try testing.expect(registry.tenant_api_key_mw.scope_host == erase(&deps.resolver));
}

test "should hand the webhook verifier the approval signing secret" {
    var deps = Deps.init();

    const registry = serve_boot.buildRegistry(deps.asRegistryDeps());

    try testing.expectEqualStrings(APPROVAL_SECRET, registry.webhook_hmac_mw.secret);
}

test "should build no verifier when OIDC is disabled" {
    var cfg: @import("../config/runtime.zig").ServeConfig = undefined;
    cfg.oidc_enabled = false;

    // The disabled path must return before touching any other config field —
    // a boot with OIDC off carries no issuer, audience, or JWKS URL to read.
    try testing.expect(try serve_boot.initOidc(testing.allocator, &cfg) == null);
}
