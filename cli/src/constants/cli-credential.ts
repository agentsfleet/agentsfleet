// Credential shape facts shared with the daemon. Every value here mirrors a
// declaration in `rustd/crates/afd_auth/` — the machine label in
// `rustd/crates/afd_tenant/` — and must not drift from it: the minting
// endpoint and this client validate the same strings, so a rename on
// either side that lands alone turns into a refusal at runtime (RULE UFS).

// Mirrors CLI_CREDENTIAL_PREFIX in rustd/crates/afd_auth/src/credential.rs.
// The durable per-user credential `agentsfleet login` mints and persists.
export const CLI_CREDENTIAL_PREFIX = "afc_" as const;

// Mirrors TENANT_API_KEY_PREFIX in rustd/crates/afd_auth/src/credential.rs.
// A tenant key is a different credential class, not a session token: it is
// accepted on load (`--token` persists one) and refused later, by any route
// that requires a user principal.
export const TENANT_KEY_PREFIX = "agt_t" as const;

// Mirrors BODY_HEX_LEN in rustd/crates/afd_auth/src/authenticate.rs — 32
// random bytes rendered as lower-case hex.
export const CLI_CREDENTIAL_BODY_LEN = 64;

// Prefix plus hex body — the total the shape check in that module measures
// against, which composes the two rather than naming a constant of its own.
export const CLI_CREDENTIAL_TOTAL_LEN =
  CLI_CREDENTIAL_PREFIX.length + CLI_CREDENTIAL_BODY_LEN;

// Mirrors the pair `CredentialKind::of` + `accepts_shape` in that module:
// exact length, exact prefix, and a body of lower-case hex. Anchored, so a
// value carrying trailing bytes is refused rather than matched on its head.
export const CLI_CREDENTIAL_PATTERN = new RegExp(
  `^${CLI_CREDENTIAL_PREFIX}[0-9a-f]{${CLI_CREDENTIAL_BODY_LEN}}$`,
);

// Mirrors MACHINE_NAME_MAX in
// rustd/crates/afd_tenant/src/cli_credential/machine.rs, which counts
// characters rather than bytes — so a non-ASCII name gets the same 64.
export const MAX_MACHINE_NAME_LEN = 64;

// No longer a mirror: the daemon dropped the letters/digits/hyphen/
// underscore/dot grammar, and that same machine.rs now bounds the length,
// trims the ends, and accepts any Unicode in between. The narrowing stays
// here as the CLI's own taste for the label it DERIVES from a hostname —
// the operator never types this value, so a plain one keeps the credential
// list readable. Expressed as the grammar's complement because the derive
// substitutes rather than refuses.
export const MACHINE_NAME_DISALLOWED = /[^a-zA-Z0-9._-]/g;

// Substituted for each byte outside the grammar, so a hostname carrying a
// space or an accent still yields a label the endpoint accepts.
export const MACHINE_NAME_REPLACEMENT = "-" as const;

// Used only when a host reports a name that sanitizes to nothing at all.
// Within the grammar, so it survives the same validation as a real hostname.
export const FALLBACK_MACHINE_NAME = "unknown-machine" as const;
