import { describe, expect, test } from "bun:test";

import { ensurePlatformSecretsSeeded } from "./acceptance/fixtures/platform-secrets.ts";

const COMMAND_TIMEOUT_MS = 30_000;
const INITIAL_REMAINING_MS = 40_000;
const ELAPSED_PER_COMMAND_MS = 1_000;

describe("platform acceptance secret seeding", () => {
  test("creates all required names without force", async () => {
    const calls: ReadonlyArray<string>[] = [];
    await ensurePlatformSecretsSeeded(
      { AGENTSFLEET_API_URL: "https://api.test" },
      () => COMMAND_TIMEOUT_MS,
      async (args) => {
        calls.push(args);
        return { code: 0, stdout: '{"status":"stored"}', stderr: "" };
      },
    );
    expect(calls.map((args) => args[2])).toEqual(["fly", "upstash", "slack", "github"]);
    expect(calls.every((args) => !args.includes("--force"))).toBe(true);
    expect(calls.every((args) => args.includes("--json"))).toBe(true);
  });

  test("stops immediately when a seed command fails", async () => {
    let calls = 0;
    await expect(ensurePlatformSecretsSeeded({}, () => COMMAND_TIMEOUT_MS, async () => {
      calls += 1;
      return { code: 3, stdout: "", stderr: "vault unavailable" };
    })).rejects.toThrow("platform secret seed failed for fly: vault unavailable");
    expect(calls).toBe(1);
  });

  test("bounds each command by the latest remaining deadline", async () => {
    let remaining = INITIAL_REMAINING_MS;
    const observedTimeouts: number[] = [];
    await ensurePlatformSecretsSeeded({}, () => {
      remaining -= ELAPSED_PER_COMMAND_MS;
      return remaining;
    }, async (_args, opts) => {
      observedTimeouts.push(opts.timeoutMs);
      return { code: 0, stdout: "", stderr: "" };
    });
    expect(observedTimeouts).toEqual([
      INITIAL_REMAINING_MS - ELAPSED_PER_COMMAND_MS,
      INITIAL_REMAINING_MS - (ELAPSED_PER_COMMAND_MS * 2),
      INITIAL_REMAINING_MS - (ELAPSED_PER_COMMAND_MS * 3),
      INITIAL_REMAINING_MS - (ELAPSED_PER_COMMAND_MS * 4),
    ]);
  });
});
