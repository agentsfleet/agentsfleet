import { afterAll } from "bun:test";

import { revokeMintedSessions } from "./fixtures/clerk-admin.ts";

afterAll(async () => {
  await revokeMintedSessions();
});
