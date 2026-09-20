// Flag validation refuses over the wire's behalf: every case here is a request
// the server would reject, caught before it leaves the process. The messages
// are asserted because they are what the operator reads — and because the
// three a single flag can produce have to read as one family.

import { describe, expect, test } from "bun:test";

import { runCli } from "../src/cli.ts";

const EXIT_VALIDATION = 4;

const reject = async (argv: ReadonlyArray<string>): Promise<{ code: number; err: string }> => {
  const chunks: string[] = [];
  const code = await runCli([...argv], {
    stdout: { write: () => true, isTTY: false },
    stderr: { write: (c: string) => { chunks.push(c); return true; }, isTTY: false },
    env: { AGENTSFLEET_API_KEY: "afk_unit_test", NO_COLOR: "1" },
  });
  return { code, err: chunks.join("") };
};

describe("--base-url refuses anything that is not an https URL", () => {
  test("a plain http URL is refused", async () => {
    const { code, err } = await reject([
      "secret", "create", "a-secret", "--provider", "custom", "--base-url", "http://example.test",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an https URL");
  });

  // `https://` passes the prefix check and then fails to parse — the branch a
  // prefix test alone would never reach.
  test("a string that starts https:// but cannot be parsed is refused", async () => {
    const { code, err } = await reject([
      "secret", "create", "a-secret", "--provider", "custom", "--base-url", "https://",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an https URL");
  });

  test("a well-formed https URL passes validation", async () => {
    const { code, err } = await reject([
      "secret", "create", "a-secret", "--provider", "openai-compatible",
      "--base-url", "https://example.test", "--model", "m", "--api-key", "k",
    ]);
    expect(err).not.toContain("must be an https URL");
    // Past the flag check it reaches the wire, which this test has no server
    // for — so the run fails, but never on the URL's shape.
    expect(code).not.toBe(EXIT_VALIDATION);
  });
});

describe("--limit refuses out-of-range and non-numeric values in one voice", () => {
  test("below the floor names the floor", async () => {
    const { code, err } = await reject(["list", "--limit", "0"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be ≥ 1");
  });

  test("above the cap names the cap", async () => {
    const { code, err } = await reject(["list", "--limit", "9999"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be ≤ 200");
  });

  test("a non-numeric value is refused as an integer problem", async () => {
    const { code, err } = await reject(["list", "--limit", "abc"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an integer");
  });

  test("each command carries its own cap", async () => {
    const { err } = await reject(["logs", "--limit", "9999"]);
    expect(err).toContain("must be ≤ 500");
  });
});

describe("id flags refuse anything that is not a canonical uuidv7", () => {
  test("a non-uuid is refused before the request", async () => {
    const { code, err } = await reject(["list", "--workspace-id", "not-a-uuid"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("uuidv7");
  });

  test("an uppercase uuidv7 is refused, because canonical form is lowercase", async () => {
    const { code } = await reject([
      "list", "--workspace-id", "0192A3B4-C5D6-7E8F-9012-345678901234",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
  });
});
