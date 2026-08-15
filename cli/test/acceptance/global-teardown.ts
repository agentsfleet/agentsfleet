import { afterAll } from "bun:test";

import { revokeMintedSessions } from "./fixtures/clerk-admin.ts";
import { revokeHydratedCliCredentials } from "./fixtures/workspace-hydration.ts";
import { resolveAcceptanceEnv } from "./global-setup.ts";

afterAll(async () => {
  await revokeHydratedCliCredentials(resolveAcceptanceEnv().apiUrl);
  await revokeMintedSessions();
});
