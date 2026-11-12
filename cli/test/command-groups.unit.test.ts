// The grouped command surface: every command is under its resource, the old
// root-level spellings are gone, and setup chat never turns into a runtime
// message.

import { describe, expect, test } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { buildCommandTree } from "../src/program/tree/index.ts";
import { walkCommandPath, type CommandNode } from "../src/program/tree/resolve-path.ts";
import { bufferStream, cliEnv, withFreshStateDir } from "./helpers-cli-state.ts";
import { COMMAND_PARITY, FLEET_ID, WORKSPACE_ID } from "./fixtures/command-parity.ts";

const API_URL = "http://stub.test";
const NOT_FOUND = JSON.stringify({ type: "about:blank", title: "Not Found", status: 404, code: "UZ-TEST-404", detail: "stub" });
const EXIT_USAGE = 4;

interface Recorded {
  readonly code: number;
  readonly out: string;
  readonly err: string;
  readonly calls: ReadonlyArray<string>;
  readonly bodies: ReadonlyArray<string>;
}

// Each run gets a fresh state dir holding one active workspace and an env
// key, so workspace-scoped commands resolve and nothing reads a real login.
const record = (argv: ReadonlyArray<string>): Promise<Recorded> =>
  withFreshStateDir(async (dir) => {
    await fs.writeFile(
      path.join(dir, "workspaces.json"),
      JSON.stringify({ current_workspace_id: WORKSPACE_ID, items: [{ workspace_id: WORKSPACE_ID }] }),
    );
    const calls: string[] = [];
    const bodies: string[] = [];
    const fetchImpl = async (url: string, init?: RequestInit) => {
      calls.push(`${(init?.method ?? "GET").toUpperCase()} ${new URL(url).pathname}`);
      bodies.push(typeof init?.body === "string" ? init.body : "");
      return new Response(NOT_FOUND, { status: 404, headers: { "content-type": "application/problem+json" } });
    };
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli([...argv], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({ AGENTSFLEET_API_URL: API_URL, AGENTSFLEET_API_KEY: "agt_t_groups", NO_COLOR: "1" }),
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    return { code, out: out.read(), err: err.read(), calls, bodies };
  });

const helpRows = (text: string): ReadonlyArray<string> =>
  text.split("\n").flatMap((line) => {
    const match = /^ {2}([a-z][a-z-]*)\s{2,}/.exec(line);
    return match?.[1] === undefined ? [] : [match[1]];
  });

const tree = buildCommandTree() as unknown as CommandNode;

describe("canonical_groups_share_help_and_handlers", () => {
  for (const row of COMMAND_PARITY) {
    test(`${row.argv.join(" ")} is listed under its group and reaches its handler`, async () => {
      const commandPath = walkCommandPath(tree, row.argv).path;
      const leaf = commandPath.at(-1) ?? "";
      expect(leaf.length).toBeGreaterThan(0);
      const help = await record([...commandPath.slice(0, -1), "--help"]);
      expect(helpRows(help.out)).toContain(leaf);

      const run = await record(row.argv);
      expect(run.calls).toEqual(row.calls);
    });
  }
});

// The root verbs the regroup moved. None keeps an alias.
const REMOVED_ROOT_VERBS = [
  "list", "status", "stop", "resume", "kill", "delete", "steer", "logs", "events",
  "approvals", "schedule", "memory", "grant", "tenant", "billing", "secret", "connector", "api-key",
] as const;

const REMOVED_SPELLING = new RegExp(`agentsfleet (${REMOVED_ROOT_VERBS.join("|")})\\b`);

describe("removed_root_verbs_are_rejected", () => {
  test.each(REMOVED_ROOT_VERBS.map((verb) => [verb]))("%s is an unknown command", async (verb) => {
    const json = await record(["--json", verb]);
    expect(json.code).toBe(EXIT_USAGE);
    expect(JSON.parse(json.err).error.code).toBe("UNKNOWN_COMMAND");
    // A root typo reads the caller's scopes for its suggestions, and nothing else.
    expect(json.calls).toEqual([`GET /v1/users/me`]);

    const text = await record([verb]);
    expect(text.code).toBe(EXIT_USAGE);
    expect(text.err).not.toMatch(REMOVED_SPELLING);
  });

  test("text after -- reaches steer as the message, not as a flag", async () => {
    const run = await record(["fleet", "steer", FLEET_ID, "--", "--json"]);
    expect(run.calls).toEqual([`POST /v1/workspaces/${WORKSPACE_ID}/fleets/${FLEET_ID}/messages`]);
    expect(JSON.parse(run.bodies[0] ?? "{}").message).toBe("--json");
  });
});

describe("setup_chat_never_calls_runtime_steer", () => {
  const drafts = `/v1/workspaces/${WORKSPACE_ID}/drafts`;
  const messages = `/v1/workspaces/${WORKSPACE_ID}/fleets/${FLEET_ID}/messages`;

  test("chat --fleet edits setup through drafts only", async () => {
    const run = await record(["chat", "--fleet", FLEET_ID, "--setup-revision", "1", "--model", "authoring-default", "--message", "x", "--no-input"]);
    expect(run.calls.length).toBeGreaterThan(0);
    for (const call of run.calls) {
      expect(call.split(" ")[1]?.startsWith(drafts)).toBe(true);
      expect(call).not.toContain(messages);
    }
  });

  test("fleet steer admits a runtime message and touches no draft", async () => {
    const run = await record(["fleet", "steer", FLEET_ID, "x"]);
    expect(run.calls).toEqual([`POST ${messages}`]);
    expect(run.calls.some((call) => call.includes(drafts))).toBe(false);
  });
});
