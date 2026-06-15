// Auth-guard integration tests — the legacy success-path login tests
// in this file targeted the pre-device-flow `{status: complete, token}`
// shape that the post-Slice-3 server no longer serves. They were removed
// when login.ts was rewritten onto the new device flow. The new-flow
// equivalents (success path, expired, timeout, workspace hydration,
// first-time bootstrap) belong in the dimension batch (D20/D22/D24)
// landing later — they need a real ECDH mock-encrypt helper plus an
// Input service fake that the runCli wrapper doesn't yet expose.
//
// What stays in this file is the auth-guard short-circuit suite: those
// assertions are independent of the login flow and continue to pass.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { bufferStream, withFreshStateDir } from "./helpers-cli-state.ts";

describe("first-time user onboarding", () => {
  test("an auth-required command without credentials exits 1 with a clear message", async () => {
    await withFreshStateDir(async () => {
      // No mock API needed — the auth guard short-circuits before any fetch.
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream, stderr: err.stream, env: {},
      });
      expect(code).toBe(1);
      expect(err.read()).toMatch(/not authenticated/i);
    });
  });

  test("corrupt credentials.json is treated as no-token; auth guard fires cleanly", async () => {
    await withFreshStateDir(async (stateDir) => {
      // Pre-seed a broken credentials.json. loadCredentials' readJson catches
      // SyntaxError and falls back to the empty sentinel — the customer is
      // effectively logged out and the auth guard short-circuits doctor.
      await fs.mkdir(stateDir, { recursive: true });
      await fs.writeFile(path.join(stateDir, "credentials.json"), "{ this is not json", { mode: 0o600 });
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream, stderr: err.stream, env: {},
      });
      expect(code).toBe(1);
      expect(err.read()).toMatch(/not authenticated/i);
      // The corrupt file is preserved untouched; loadCredentials never overwrites
      // on parse failure (only saveCredentials writes, and we didn't save anything).
      const stillBroken = await fs.readFile(path.join(stateDir, "credentials.json"), "utf8");
      expect(stillBroken).toBe("{ this is not json");
    });
  });
});
