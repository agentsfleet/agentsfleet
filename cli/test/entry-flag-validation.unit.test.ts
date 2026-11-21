// Flag validation refuses a value whose shape is wrong before it leaves the
// process. How large a value may be is the server's call: a value past a bound
// the client once copied goes out, and the server's refusal is what the
// operator reads. The messages are asserted because they are what the operator
// reads — and because the two a single flag can produce read as one family.

import { describe, expect, test } from "bun:test";
import { Cause } from "effect";

import { runCli } from "../src/cli.ts";
import { wsFleetsPath, wsFleetTurnsPath } from "../src/lib/api-paths.ts";
import { exitCodeForFailure, isLibraryUsageError } from "../src/program/entry/exit-code.ts";
import { cliEnv, withAuthedStateDir } from "./helpers-cli-state.ts";
import { jsonResponse, withMockApi, type MockCall } from "./helpers-mock-api.ts";

const EXIT_VALIDATION = 4;
const WS_ID = "01900000-0000-7000-8000-000000b00b00";
const FLEET_ID = "01900000-0000-7000-8000-000000c0ffee";
// Past every `--limit` ceiling the client once copied (200 for lists, 500 for turns).
const PAST_RETIRED_CAP = "9999";
const SERVER_REFUSAL = { error_code: "UZ-REQ-001", user_message: "limit is above this route's ceiling" };

test("an unexpected runtime defect exits as a failure without posing as a usage error", () => {
  const cause = Cause.die(new Error("unexpected runtime failure"));
  expect(exitCodeForFailure(cause)).toBe(1);
  expect(isLibraryUsageError(cause)).toBe(false);
});

const reject = async (argv: ReadonlyArray<string>): Promise<{ code: number; err: string }> => {
  const chunks: string[] = [];
  const code = await runCli([...argv], {
    stdout: { write: () => true, isTTY: false },
    stderr: { write: (c: string) => { chunks.push(c); return true; }, isTTY: false },
    env: { AGENTSFLEET_API_KEY: "afk_unit_test", NO_COLOR: "1" },
  });
  return { code, err: chunks.join("") };
};

// The server refuses the page size itself; the CLI shows that refusal.
const toServer = (argv: ReadonlyArray<string>, route: string): Promise<{ code: number; err: string; calls: MockCall[] }> =>
  withAuthedStateDir({ workspaceId: WS_ID }, () =>
    withMockApi({ [route]: () => jsonResponse(400, SERVER_REFUSAL) }, async (apiUrl, calls) => {
      const chunks: string[] = [];
      const code = await runCli([...argv], {
        stdout: { write: () => true, isTTY: false },
        stderr: { write: (c: string) => { chunks.push(c); return true; }, isTTY: false },
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }),
      });
      return { code, err: chunks.join(""), calls };
    }));

describe("--base-url refuses anything that is not an https URL", () => {
  test("a plain http URL is refused", async () => {
    const { code, err } = await reject([
      "secrets", "create", "a-secret", "--provider", "custom", "--base-url", "http://example.test",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an https URL");
  });

  // `https://` passes the prefix check and then fails to parse — the branch a
  // prefix test alone would never reach.
  test("a string that starts https:// but cannot be parsed is refused", async () => {
    const { code, err } = await reject([
      "secrets", "create", "a-secret", "--provider", "custom", "--base-url", "https://",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an https URL");
  });

  test("a well-formed https URL passes validation", async () => {
    const { code, err } = await reject([
      "secrets", "create", "a-secret", "--provider", "openai-compatible",
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
    const { code, err } = await reject(["fleet", "list", "--limit", "0"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be ≥ 1");
  });

  test("a value past the retired list cap reaches the server, whose refusal is shown", async () => {
    const { code, err, calls } = await toServer(["fleet", "list", "--limit", PAST_RETIRED_CAP], `GET ${wsFleetsPath(WS_ID)}`);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.search).toContain(`limit=${PAST_RETIRED_CAP}`);
    expect(code).not.toBe(0);
    expect(err).toContain(SERVER_REFUSAL.user_message);
    expect(err).not.toContain("must be ≤");
  });

  test("a non-numeric value is refused as an integer problem", async () => {
    const { code, err } = await reject(["fleet", "list", "--limit", "abc"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("must be an integer");
  });

  test("a value past the retired turns cap reaches the server too", async () => {
    const { err, calls } = await toServer(["fleet", "logs", FLEET_ID, "--limit", PAST_RETIRED_CAP],
      `GET ${wsFleetTurnsPath(WS_ID, FLEET_ID)}`);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.search).toContain(`limit=${PAST_RETIRED_CAP}`);
    expect(err).toContain(SERVER_REFUSAL.user_message);
  });
});

describe("id flags refuse anything that is not a canonical uuidv7", () => {
  test("a non-uuid is refused before the request", async () => {
    const { code, err } = await reject(["fleet", "list", "--workspace", "not-a-uuid"]);
    expect(code).toBe(EXIT_VALIDATION);
    expect(err).toContain("uuidv7");
  });

  test("an uppercase uuidv7 is refused, because canonical form is lowercase", async () => {
    const { code } = await reject([
      "fleet", "list", "--workspace", "0192A3B4-C5D6-7E8F-9012-345678901234",
    ]);
    expect(code).toBe(EXIT_VALIDATION);
  });
});
