// Credential shape facts shared with the daemon. Every value here mirrors a
// declaration in `src/agentsfleetd/auth/` and must not drift from it: the
// minting endpoint and this client validate the same strings, so a rename on
// either side that lands alone turns into a refusal at runtime (RULE UFS).

// Mirrors PREFIX in src/agentsfleetd/auth/cli_credential.zig. The durable
// per-user credential `agentsfleet login` mints and persists.
export const CLI_CREDENTIAL_PREFIX = "afc_" as const;

// Mirrors TENANT_KEY_PREFIX in
// src/agentsfleetd/auth/middleware/tenant_api_key.zig. A tenant key is a
// different credential class, not a session token: it is accepted on load
// (`--token` persists one) and refused later, by any route that requires a
// user principal.
export const TENANT_KEY_PREFIX = "agt_t" as const;

// Mirrors BODY_LEN in src/agentsfleetd/auth/cli_credential.zig — 32 random
// bytes rendered as lower-case hex.
export const CLI_CREDENTIAL_BODY_LEN = 64;

// Mirrors TOTAL_LEN in that module: prefix plus hex body.
export const CLI_CREDENTIAL_TOTAL_LEN =
  CLI_CREDENTIAL_PREFIX.length + CLI_CREDENTIAL_BODY_LEN;

// Mirrors looksWellFormed in that module: exact length, exact prefix, and a
// body of lower-case hex. Anchored, so a value carrying trailing bytes is
// refused rather than matched on its head.
export const CLI_CREDENTIAL_PATTERN = new RegExp(
  `^${CLI_CREDENTIAL_PREFIX}[0-9a-f]{${CLI_CREDENTIAL_BODY_LEN}}$`,
);

// Mirrors MAX_MACHINE_NAME_LEN in src/agentsfleetd/auth/cli_credential.zig.
export const MAX_MACHINE_NAME_LEN = 64;

// Mirrors the grammar isValidMachineName accepts in that same module —
// letters, digits, hyphen, underscore, dot. Expressed as its complement
// because the client's job is to replace what the endpoint would refuse.
export const MACHINE_NAME_DISALLOWED = /[^a-zA-Z0-9._-]/g;

// Substituted for each byte outside the grammar, so a hostname carrying a
// space or an accent still yields a label the endpoint accepts.
export const MACHINE_NAME_REPLACEMENT = "-" as const;

// Used only when a host reports a name that sanitizes to nothing at all.
// Within the grammar, so it survives the same validation as a real hostname.
export const FALLBACK_MACHINE_NAME = "unknown-machine" as const;
