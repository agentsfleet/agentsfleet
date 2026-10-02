// The workspace name bounds and checks the create dialog shares, and the account-role vocabulary. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export const WORKSPACE_NAME_MAX_CODEPOINTS = 128;

const ASCII_EDGE_WHITESPACE_PATTERN =
  /^[\u0009-\u000d\u0020]+|[\u0009-\u000d\u0020]+$/gu;

const UNICODE_WHITESPACE_ONLY_PATTERN =
  /^[\u00a0\u1680\u2000-\u200a\u202f\u205f\u3000]*$/u;

const WORKSPACE_NAME_UNSAFE_PATTERN =
  /[\u0000-\u001f\u007f-\u009f\u061c\u200e-\u200f\u2028-\u202e\u2066-\u2069]/u;

export function trimWorkspaceName(name: string): string {
  return name.replace(ASCII_EDGE_WHITESPACE_PATTERN, "");
}

export function hasWorkspaceNameContent(name: string): boolean {
  return name.length > 0 && !UNICODE_WHITESPACE_ONLY_PATTERN.test(name);
}

export function isWorkspaceNameSafe(name: string): boolean {
  return !WORKSPACE_NAME_UNSAFE_PATTERN.test(name);
}

/** The caller's role in a workspace's account. Mirrors `ROLE_OWNER` and
 * `ROLE_MEMBER` in `rustd/crates/afd_tenant/src/workspace/access.rs`. */
export const ACCOUNT_ROLE = {
  owner: "owner",
  member: "member",
} as const;

export type AccountRole = (typeof ACCOUNT_ROLE)[keyof typeof ACCOUNT_ROLE];
