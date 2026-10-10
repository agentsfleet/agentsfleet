// A row in the by-identifier matrix that stopped validating client-side
// would send a malformed id to the API. Every row is driven through the
// command tree against a mock API, signed in, so the refusal is observed
// rather than read off the table.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { validateRequiredId } from "../src/lib/id.ts";
import { INVALID_ID_SAMPLES, REQUIRES_IDENTIFIER } from "./acceptance/fixtures/command-matrix.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { authedScope, withMockApi } from "./helpers-fleet-install.ts";

const JSON_FLAG = "--json";
const EXIT_OK = 0;

const refuseOffline = (argv: readonly string[]) =>
  authedScope(() =>
    withMockApi({}, async (apiUrl, calls) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli([...argv, JSON_FLAG], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      return { code, output: `${out.read()}${err.read()}`, requests: calls.length };
    }),
  );

describe("every identifier-taking verb refuses a malformed id before any request", () => {
  const cases = REQUIRES_IDENTIFIER.flatMap((row) =>
    INVALID_ID_SAMPLES.map((sample) => ({ label: `${row.args.join(" ")} "${sample}"`, row, sample })),
  );

  test.each(cases)("$label is refused without a request", async ({ row, sample }) => {
    const result = await refuseOffline([...row.args, sample]);
    const refusal = validateRequiredId(sample, row.argName);
    if (refusal.ok) throw new Error(`sample ${sample} passes the id validator`);

    expect(result.code).not.toBe(EXIT_OK);
    expect(result.requests).toBe(0);
    expect(result.output).toContain(refusal.message);
  });
});
