/**
 * Argument-rejection acceptance sweep — the deterministic half of the CLI's
 * negative surface.
 *
 * Every row runs the real built binary against an unroutable API base URL and
 * an empty state directory, so a rejection that leaked a network call would
 * surface as a connection error instead of the expected stem. Nothing here
 * needs credentials or a live target, which is the point: before M171 the
 * only proof that `events` and `steer` reject a missing identifier lived in
 * the live lane, so `make test-unit-all` never ran it.
 *
 * The claim under test is uniformity. One shape (`✕ error:` + `Suggestion:`),
 * one exit code (validation), one stream (stderr for failure, stdout for
 * help) — whether commander rejected the invocation at parse time or a
 * handler rejected it after.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import { runFleetctl, composeEnv } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";
import {
  EXAMPLE_FLEET_ID,
  GROUP_NODES,
  HANDLER_VALIDATED_REQUIRED_FLAG,
  MALFORMED_ID_INVOCATIONS,
  MISSING_OPTION_VALUE,
  MISSING_REQUIRED_OPTION,
  REQUIRES_POSITIONAL_ARG,
} from "./fixtures/command-matrix.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const ERROR_GLYPH = "✕";
const ERROR_STEM = "error:";
const SUGGESTION_STEM = "Suggestion:";
// Deliberately narrow: a bare /connect/ also matches the word "connector",
// which is a command name, not evidence of a socket.
const CONNECTION_MARKERS = /ECONNREFUSED|ENOTFOUND|ETIMEDOUT|fetch failed/i;
const EXIT_VALIDATION = EXIT_CODE.ValidationError;

// A syntactically valid but never-honoured credential. The auth guard runs
// BEFORE handler-side argument validation, so an empty state directory would
// make every handler-validated row report "not authenticated" instead of the
// rejection under test. The token never leaves the process: the API base URL
// is unroutable, and every row asserts no connection was attempted.
let stubState: StubbedStateDir | null = null;

beforeAll(async () => {
  stubState = await makeStubbedStateDir();
});

afterAll(async () => {
  if (stubState) await stubState.cleanup();
});

function env(): Record<string, string> {
  if (!stubState) throw new Error("stubState not initialised");
  return composeEnv({
    AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
    AGENTSFLEET_STATE_DIR: stubState.dir,
    NO_COLOR: "1",
  });
}

interface Rejection {
  readonly code: number;
  readonly stderr: string;
  readonly stdout: string;
  readonly detailLine: string;
  readonly suggestionLine: string;
}

async function reject(args: ReadonlyArray<string>): Promise<Rejection> {
  const result = await runFleetctl([...args], { env: env() });
  const lines = result.stderr.split("\n").map((l) => l.trim()).filter(Boolean);
  const detailLine = lines.find((l) => l.includes(ERROR_STEM)) ?? "";
  const suggestionLine = lines.find((l) => l.startsWith(SUGGESTION_STEM)) ?? "";
  return { code: result.code, stderr: result.stderr, stdout: result.stdout, detailLine, suggestionLine };
}

// The shared claim every rejection row makes, whichever layer raised it.
function assertHouseShape(r: Rejection, label: string): void {
  assert.equal(r.code, EXIT_VALIDATION,
    `${label}: expected exit ${EXIT_VALIDATION}, got ${r.code}; stderr=${r.stderr}`);
  assert.ok(r.stderr.includes(ERROR_GLYPH),
    `${label}: stderr carries no ${ERROR_GLYPH} glyph; stderr=${r.stderr}`);
  assert.ok(r.detailLine, `${label}: no "${ERROR_STEM}" line; stderr=${r.stderr}`);
  assert.ok(r.suggestionLine, `${label}: no "${SUGGESTION_STEM}" line; stderr=${r.stderr}`);
  const detail = r.detailLine.slice(r.detailLine.indexOf(ERROR_STEM) + ERROR_STEM.length).trim();
  const suggestion = r.suggestionLine.slice(SUGGESTION_STEM.length).trim();
  assert.notEqual(suggestion, detail,
    `${label}: the suggestion repeats the detail instead of naming the fix`);
  assert.ok(!CONNECTION_MARKERS.test(r.stderr),
    `${label}: rejection reached the network; stderr=${r.stderr}`);
}

describe("missing required positional", () => {
  for (const row of REQUIRES_POSITIONAL_ARG) {
    const label = row.args.join(" ");
    it(`"${label}" rejects in the house shape`, async () => {
      const r = await reject(row.args);
      assertHouseShape(r, label);
      const expected = row.reportedToken ?? row.missingArgName;
      assert.ok(r.detailLine.includes(expected),
        `${label}: detail does not name ${expected}; got ${r.detailLine}`);
    });
  }
});

describe("missing option value", () => {
  for (const args of MISSING_OPTION_VALUE) {
    const label = args.join(" ");
    it(`"${label}" rejects in the house shape`, async () => {
      assertHouseShape(await reject(args), label);
    });
  }
});

describe("missing required option", () => {
  for (const args of MISSING_REQUIRED_OPTION) {
    const label = args.join(" ");
    it(`"${label}" rejects in the house shape`, async () => {
      assertHouseShape(await reject(args), label);
    });
  }
});

describe("handler-validated required flag", () => {
  for (const args of HANDLER_VALIDATED_REQUIRED_FLAG) {
    const label = args.join(" ");
    it(`"${label}" rejects in the same shape commander rows use`, async () => {
      assertHouseShape(await reject(args), label);
    });
  }
});

describe("malformed identifier", () => {
  for (const args of MALFORMED_ID_INVOCATIONS) {
    const label = args.join(" ");
    it(`"${label}" is rejected client-side in the house shape`, async () => {
      assertHouseShape(await reject(args), label);
    });
  }
});

describe("unknown command", () => {
  it("an unknown root command keeps its did-you-mean inside the house shape", async () => {
    const r = await reject(["docto"]);
    assertHouseShape(r, "docto");
    assert.match(r.stderr, /doctor/, `did-you-mean text lost; stderr=${r.stderr}`);
  });

  it("an unknown subcommand names the token and points at the group's list", async () => {
    const r = await reject(["connector", "pogo"]);
    assertHouseShape(r, "connector pogo");
    assert.match(r.stderr, /pogo/, `unrecognized token not named; stderr=${r.stderr}`);
    assert.match(r.suggestionLine, /connector --help/,
      `suggestion does not point at the group's list; got ${r.suggestionLine}`);
  });
});

describe("group nodes print help that survives a pipe", () => {
  for (const args of GROUP_NODES) {
    const label = args.join(" ");
    it(`"${label}" writes help to stdout at exit 0`, async () => {
      const result = await runFleetctl([...args], { env: env() });
      assert.equal(result.code, 0, `${label}: expected exit 0; stderr=${result.stderr}`);
      assert.ok(result.stdout.length > 0, `${label}: help body did not reach stdout`);
      assert.match(result.stdout, /Usage:/, `${label}: stdout carries no usage banner`);
      assert.equal(result.stderr.trim(), "", `${label}: help leaked to stderr: ${result.stderr}`);
    });

    // Regression: resolving this from the argv shape instead of from
    // commander's own help path sent the body to stderr the moment any
    // global flag was present.
    it(`"${label}" writes help to stdout with a global flag present`, async () => {
      const result = await runFleetctl([...args, "--json"], { env: env() });
      assert.equal(result.code, 0, `${label} --json: expected exit 0; stderr=${result.stderr}`);
      assert.ok(result.stdout.length > 0, `${label} --json: help body did not reach stdout`);
      assert.equal(result.stderr.trim(), "", `${label} --json: help leaked to stderr`);
    });

    it(`"${label} --help" matches the bare invocation`, async () => {
      const bare = await runFleetctl([...args], { env: env() });
      const flagged = await runFleetctl([...args, "--help"], { env: env() });
      assert.equal(flagged.code, 0, `${label} --help: expected exit 0`);
      assert.equal(flagged.stdout, bare.stdout,
        `${label}: bare and --help bodies differ`);
    });
  }
});

describe("JSON mode emits the machine envelope", () => {
  it("a rejected invocation answers with a parseable error envelope", async () => {
    const result = await runFleetctl(["--json", "events"], { env: env() });
    assert.equal(result.code, EXIT_VALIDATION);
    const parsed = JSON.parse(result.stderr.trim()) as {
      error?: { code?: string; message?: string };
    };
    assert.equal(parsed.error?.code, "MISSING_ARGUMENT",
      `envelope carries no stable code; stderr=${result.stderr}`);
    assert.match(String(parsed.error?.message), /fleet_id/,
      `envelope message does not name the argument; stderr=${result.stderr}`);
  });

  it("an unknown command names the token it did not recognise", async () => {
    const result = await runFleetctl(["--json", "zzzz"], { env: env() });
    const parsed = JSON.parse(result.stderr.trim()) as {
      error?: { code?: string; message?: string };
    };
    assert.equal(parsed.error?.code, "UNKNOWN_COMMAND");
    assert.match(String(parsed.error?.message), /zzzz/);
  });
});

describe("a valid invocation still fails as a transport error", () => {
  it("keeps the network exit code distinct from a rejected invocation", async () => {
    const result = await runFleetctl(["logs", "--fleet", EXAMPLE_FLEET_ID], { env: env() });
    assert.notEqual(result.code, EXIT_VALIDATION,
      `a well-formed invocation must not report as a rejected one; stderr=${result.stderr}`);
  });
});
