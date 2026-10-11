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

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv, withFreshStateDir } from "./helpers-cli-state.ts";

describe("first-time user onboarding", () => {
  test("an auth-required command without credentials exits 1 with a clear message", async () => {
    await withFreshStateDir(async () => {
      // No mock API needed — the auth guard short-circuits before any fetch.
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream, stderr: err.stream, env: cliEnv(),
      });
      expect(code).toBe(1);
      expect(err.read()).toMatch(/not authenticated/i);
    });
  });
});
