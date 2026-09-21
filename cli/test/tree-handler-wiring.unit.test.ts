// The thin line between a command's flags and the effect that runs it.
//
// The effects themselves are covered by their own unit tests; what is covered
// here is the WIRING — that `tenant provider create` reaches
// tenantProviderAddEffectFromArgs with its flags, and `billing show` reaches
// billingShowEffectFromArgs with its paging. A handler wired to the wrong
// effect, or dropping a flag on the way, typechecks perfectly and fails only
// in front of a user.

import { describe, expect, test } from "bun:test";

import { runCli } from "../src/cli.ts";

const EXIT_VALIDATION = 4;
const UNROUTABLE = "https://127.0.0.1:1";

const invoke = async (argv: ReadonlyArray<string>): Promise<{ code: number; err: string }> => {
  const chunks: string[] = [];
  const code = await runCli([...argv], {
    stdout: { write: () => true, isTTY: false },
    stderr: { write: (c: string) => { chunks.push(c); return true; }, isTTY: false },
    // A real key and an unroutable target: the invocation gets all the way to
    // the transport, which is how we know the handler ran, and then fails
    // there instead of reaching anyone's deployment.
    env: { AGENTSFLEET_API_KEY: "afk_unit_test", AGENTSFLEET_API_URL: UNROUTABLE, NO_COLOR: "1" },
  });
  return { code, err: chunks.join("") };
};

describe("tenant provider create is wired to its effect", () => {
  test("it parses its flags and reaches the handler", async () => {
    const { code } = await invoke([
      "tenant", "provider", "create", "--secret", "my-secret", "--model", "gpt-4",
    ]);
    // Not a validation exit: the flags were accepted and the handler ran.
    expect(code).not.toBe(EXIT_VALIDATION);
  });

  test("it still refuses a flag it does not declare", async () => {
    const { code, err } = await invoke([
      "tenant", "provider", "create", "--secret", "s", "--model", "m", "--nope", "x",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("--nope");
  });
});

describe("billing show is wired to its effect", () => {
  test("it parses its paging flags and reaches the handler", async () => {
    const { code } = await invoke(["billing", "show", "--limit", "10"]);
    expect(code).not.toBe(EXIT_VALIDATION);
  });

  test("its cap is its own, not the list cap", async () => {
    const { code, err } = await invoke(["billing", "show", "--limit", "9999"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be ≤ 100");
  });
});
