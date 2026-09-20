// The values the login acceptance suite's server and client halves share.
//
// Its own file because both read them: the stub answers with this identity and
// mints this credential, and the specs assert on the same values. One
// declaration, so a fixture cannot be changed on one side of the round trip.

import {
  CLI_CREDENTIAL_BODY_LEN,
  CLI_CREDENTIAL_PREFIX,
} from "../src/constants/cli-credential.ts";

export const IDENTITY = {
  user_id: "0193c5e1-0000-7000-8000-00000000abcd",
  email: "ada@example.com",
  display_name: "Ada Lovelace",
  tenant_id: "0193c5e0-0000-7000-8000-000000001234",
  tenant_name: "Ada's Workshop",
  credential: "cli_credential",
  scopes: ["fleet:read"],
} as const;

export const SESSION_ID = "sess_acceptance_e2e";
export const VERIFICATION_CODE = "424242";
export const TEST_JWT = "eyJhbGciOiJIUzI1NiJ9.acceptance-payload.sig";

// What the mint hands back. Shaped the way the client validates on load —
// the afc_ prefix and a 64-character lower-case hex body — and built by
// repetition so this file carries no high-entropy literal.
const MINTED_BODY_CHAR = "b";
export const MINTED_CREDENTIAL = `${CLI_CREDENTIAL_PREFIX}${MINTED_BODY_CHAR.repeat(CLI_CREDENTIAL_BODY_LEN)}`;
export const MINTED_CREDENTIAL_ID = "cli_cred_acceptance";

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly events: Array<{ event: string; properties: Record<string, unknown> }>;
  savedToken: string | null;
  savedSessionId: string | null;
  browserOpened: boolean;
  promptsAsked: number;
  cleared: boolean;
}

export const makeRecorder = (): Recorder => ({
  stdout: [],
  stderr: [],
  events: [],
  savedToken: null,
  savedSessionId: null,
  browserOpened: false,
  promptsAsked: 0,
  cleared: false,
});
