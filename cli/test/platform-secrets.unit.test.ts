import { describe, expect, test } from "bun:test";

import { ensurePlatformSecretsSeeded } from "./acceptance/fixtures/platform-secrets.ts";

describe("platform acceptance secret seeding", () => {
  test("creates all required names without force", async () => {
    const calls: ReadonlyArray<string>[] = [];
    await ensurePlatformSecretsSeeded(
      { AGENTSFLEET_API_URL: "https://api.test" },
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
    await expect(ensurePlatformSecretsSeeded({}, async () => {
      calls += 1;
      return { code: 3, stdout: "", stderr: "vault unavailable" };
    })).rejects.toThrow("platform secret seed failed for fly: vault unavailable");
    expect(calls).toBe(1);
  });
});
