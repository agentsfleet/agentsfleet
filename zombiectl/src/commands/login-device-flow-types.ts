// Wire-shape contracts + platform default for the device-flow login, split out
// of login-device-flow.ts to keep that module under the 350-line file cap
// (RULE FLL). Re-exported from login-device-flow.ts, so existing consumers
// (login.ts, tests) import unchanged.
//
// Server response shapes are confirmed against src/http/handlers/auth/
// sessions.zig + session_helpers.zig — see the login-device-flow.ts header for
// the per-endpoint status-code map.

export type PollOutcome =
  | { readonly status: "verification_pending" }
  | { readonly status: "expired" }
  | { readonly status: "interrupted" }
  | { readonly status: "timeout" };

export interface SessionCreatedResponse {
  readonly session_id: string;
  readonly request_id?: string;
}

export interface SessionStatusResponse {
  readonly status: "pending" | "verification_pending";
  readonly cli_public_key: string;
  readonly token_name: string;
  readonly expires_at_ms: number;
}

export interface VerifySuccessResponse {
  readonly dashboard_public_key: string;
  readonly ciphertext: string;
  readonly nonce: string;
}

// Spec-pinned platform default (no hostname leak). Falls back to a
// generic label for unknown platforms.
export const defaultTokenName = (
  platform: NodeJS.Platform = process.platform,
): string => {
  if (platform === "darwin") return "macos-cli";
  if (platform === "linux") return "linux-cli";
  if (platform === "win32") return "windows-cli";
  if (platform === "freebsd") return "freebsd-cli";
  return "cli";
};
