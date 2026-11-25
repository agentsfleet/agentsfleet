// `chat` was removed: a fleet starts with `fleet create` and is talked to with
// `fleet steer`. The name still parses, only to refuse and name both, before
// any request and whether or not anyone is signed in.

import { describe, expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { authedScope } from "./helpers-fleet-steer.ts";
import { withMockApi } from "./helpers-mock-api.ts";

const FLEET_ID = "01910000-0000-7000-8000-0000000c4a02";
const CREATE = 'agentsfleet fleet create "<message>"';
const STEER = "agentsfleet fleet steer <fleet_id|@name>";
const DETAIL = "the chat command was removed: a fleet starts with fleet create and is talked to with fleet steer";
const SUGGESTION = `start a fleet with: ${CREATE}; talk to one with: ${STEER}`;

const invoke = (argv: ReadonlyArray<string>) => withMockApi({}, async (apiUrl, calls) => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli(["chat", ...argv], { stdout: out.stream, stderr: err.stream,
    env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }) });
  return { code, out: out.read(), err: err.read(), calls };
});

const chat = (argv: ReadonlyArray<string>) => authedScope(() => invoke(argv));

describe("chat is gone", () => {
  test("chat is removed and names fleet create and fleet steer", async () => {
    for (const argv of [[], [FLEET_ID], ["@argo"], [FLEET_ID, "check it now"]]) {
      const result = await chat(argv);
      expect(result.code).toBe(4);
      expect(result.err).toContain("the chat command was removed");
      expect(result.err).toContain(CREATE);
      expect(result.err).toContain(STEER);
      expect(result.out).toBe("");
      expect(result.calls).toEqual([]);
    }
  });

  test("a signed-out chat is refused the same way, with no login asked", async () => {
    const result = await invoke([]);
    expect(result.code).toBe(4);
    expect(result.err).toContain(CREATE);
    expect(result.err).not.toContain("agentsfleet login");
    expect(result.calls).toEqual([]);
  });

  test("chat --save is refused with the new command", async () => {
    const result = await chat(["--save"]);
    expect(result.code).toBe(4);
    expect(result.err).toContain(CREATE);
    expect(result.out).toBe("");
    expect(result.calls).toEqual([]);
  });

  test("--json carries the refusal as a structured error naming both commands", async () => {
    const result = await chat(["--json", "--save"]);
    expect(result.code).toBe(4);
    expect(JSON.parse(result.err)).toEqual({ error: { code: "INVALID_ARGUMENT", message: DETAIL, suggestion: SUGGESTION } });
  });

  test.each([
    [["--review"]],
    [["--no-input"]],
    [["--definition", "setup.json"]],
    [["--model", "owner/runtime"]],
    [["--message", "hello"]],
    [["--fleet", FLEET_ID]],
    [[FLEET_ID, "--apply", "--review-digest", "digest"]],
  ] as const)("the draft chat's %p is refused as chat itself, before any request", async (argv) => {
    const result = await chat(argv);
    expect(result.code).toBe(4);
    expect(result.err).toContain("the chat command was removed");
    expect(result.calls).toEqual([]);
  });
});
