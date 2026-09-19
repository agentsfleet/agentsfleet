/**
 * `agentsfleet whoami` — the end-to-end walk, deterministic half.
 *
 * The real built binary, spawned as a subprocess, against an unroutable API
 * base URL and a state directory this file owns. Nothing here needs a live
 * target, which is the point: the case that matters most is the one a person
 * hits when they have NOT logged in, and that answer must come from the local
 * credential store without a request. A row that leaked a network call would
 * surface as a connection error instead of the expected sentence.
 *
 * The signed-in walk is live-lane work and rides the read-only sweep in
 * `lifecycle-with-token.spec.ts` through the `whoami --json` row in
 * `fixtures/command-matrix.ts`, where a real credential and a real server
 * exist to answer with a real person.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import {
  makeEmptyStateDirSync,
  makeStubbedStateDir,
  type StubbedStateDir,
} from "./fixtures/state-dir.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const LOGGED_OUT_DIR = makeEmptyStateDirSync();
const LOGIN_HINT = "agentsfleet login";
const EXIT_AUTH = EXIT_CODE.AuthError;

let stubState: StubbedStateDir | null = null;

beforeAll(async () => {
  stubState = await makeStubbedStateDir();
});

afterAll(async () => {
  if (stubState) await stubState.cleanup();
});

const loggedOutEnv = (): Record<string, string> =>
  composeEnv({
    AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
    AGENTSFLEET_STATE_DIR: LOGGED_OUT_DIR,
    NO_COLOR: "1",
  });

const signedInEnv = (): Record<string, string> => {
  if (!stubState) throw new Error("stubState not initialised");
  return composeEnv({
    AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
    AGENTSFLEET_STATE_DIR: stubState.dir,
    NO_COLOR: "1",
  });
};

describe("agentsfleet whoami", () => {
  it("is offered in the root help, where somebody would look for it", async () => {
    const result = await runFleetctl(["--help"], { env: loggedOutEnv() });

    assert.equal(result.code, 0);
    assert.match(result.stdout, /whoami/, "the command appears in the root help");
  });

  it("names the command that fixes it when nothing is signed in", async () => {
    const result = await runFleetctl(["whoami"], { env: loggedOutEnv() });

    assert.equal(result.code, EXIT_AUTH, "a logged-out read is an auth failure");
    assert.match(result.stderr, new RegExp(LOGIN_HINT.replace(" ", "\\s")));
    assert.equal(result.stdout.trim(), "", "nothing is written to stdout");
  });

  it("refuses a stored credential that no longer loads, without asking the server", async () => {
    // The pre-action guard reads the raw stored string; the credentials service
    // refuses a value that is not shaped like a credential. A `credentials.json`
    // holding a stale session token therefore clears the guard and reaches the
    // command, which must still report it as signed out rather than sending a
    // request it knows will fail.
    const stale = await makeStubbedStateDir({ token: "header.payload.signature" });
    try {
      const result = await runFleetctl(["whoami"], {
        env: composeEnv({
          AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
          AGENTSFLEET_STATE_DIR: stale.dir,
          NO_COLOR: "1",
        }),
      });

      assert.equal(result.code, EXIT_AUTH);
      assert.match(result.stderr, new RegExp(LOGIN_HINT.replace(" ", "\\s")));
      assert.doesNotMatch(
        `${result.stdout}${result.stderr}`,
        /ECONNREFUSED|ENOTFOUND|ETIMEDOUT|fetch failed/i,
        "an unloadable credential is this machine's answer, not the server's",
      );
    } finally {
      await stale.cleanup();
    }
  });

  it("makes no request when it has nothing to send", async () => {
    const result = await runFleetctl(["whoami"], { env: loggedOutEnv() });

    // The base URL is unroutable, so a request would surface here. Its absence
    // is the assertion: the refusal is this machine's answer, not the server's.
    assert.doesNotMatch(
      `${result.stdout}${result.stderr}`,
      /ECONNREFUSED|ENOTFOUND|ETIMEDOUT|fetch failed/i,
      "a logged-out whoami must not dial the API to find out it is logged out",
    );
  });

  it("answers a machine-readable refusal when asked for JSON, logged out", async () => {
    const result = await runFleetctl(["whoami", "--json"], { env: loggedOutEnv() });

    // The pre-action guard answers this one, and its envelope is the shape every
    // other command's refusal carries — on stderr, because a failure belongs
    // there whatever its format. That is the assertion: a script reading
    // `whoami --json` finds its answer on the stream the CLI always uses, and
    // stdout stays clean so a pipe carries identities and nothing else.
    assert.equal(result.code, EXIT_AUTH);
    assert.equal(result.stdout.trim(), "", "a refusal never writes to stdout");
    const payload = JSON.parse(result.stderr) as { error?: Record<string, unknown> };
    assert.equal(payload.error?.["code"], "AUTH_REQUIRED");
    assert.match(String(payload.error?.["message"]), new RegExp(LOGIN_HINT.replace(" ", "\\s")));
  });

  it("reaches the network once a credential exists, and reports the outage", async () => {
    const result = await runFleetctl(["whoami"], { env: signedInEnv() });

    // The mirror of the row above: holding a credential, the command DOES ask,
    // so the unroutable target is what fails. A logged-out refusal here would
    // mean the credential on disk was never read.
    assert.notEqual(result.code, 0);
    assert.doesNotMatch(
      result.stderr,
      new RegExp(LOGIN_HINT.replace(" ", "\\s")),
      "a terminal holding a credential is not told to log in; it is told the target is unreachable",
    );
  });
});
