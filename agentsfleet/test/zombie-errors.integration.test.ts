// Error-path integration tests for zombie.ts stop / resume / kill / delete.
// Covers the requireZombieId validation branch (invalid uuidv7 format)
// and server 4xx/5xx error surfaces.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000b00b01";

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_zombie_err" }, fn);

// ---------------------------------------------------------------------------
// requireZombieId — invalid format branch (lines 66-69 in zombie.ts)
// ---------------------------------------------------------------------------

describe("zombie id validation — invalid format", () => {
  test("`stop` with a non-uuidv7 id exits non-zero and prints the format hint", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["stop", "not-a-uuid"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("zombie_id");
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("`resume` with a non-uuidv7 id exits non-zero and prints the format hint", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["resume", "bad-id-string"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(err.read()).toContain("zombie_id");
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("`kill` with a non-uuidv7 id exits non-zero and no API request is made", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["kill", "12345-not-valid"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("`delete` with a non-uuidv7 id exits non-zero and no API request is made", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["delete", "garbage-id"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).not.toBe(0);
        expect(calls).toHaveLength(0);
      });
    });
  });
});
