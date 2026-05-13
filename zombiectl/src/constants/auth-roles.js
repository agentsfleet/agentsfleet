// Tenant role names — the wire-level enum the server emits in JWT
// claims and accepts on role-gated routes. Mirrors src/auth/roles.zig.
//
// RULE UFS.

export const ROLE_ADMIN = "admin";
export const ROLE_USER = "user";
