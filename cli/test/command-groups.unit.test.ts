// The grouped command surface: every command is under its resource, the old
// root-level spellings are gone, and setup chat never turns into a runtime
// message.

import { describe, expect, test } from "bun:test";
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
    await Bun.write(
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

// Spellings removed inside a group, with no alias: each is refused before any
// request but the scope read a typo makes for its suggestions.
const REMOVED_GROUP_SPELLINGS: ReadonlyArray<readonly [ReadonlyArray<string>, string]> = [
  [["fleet", "stop", FLEET_ID], "UNKNOWN_COMMAND"],
  [["fleet", "resume", FLEET_ID], "UNKNOWN_COMMAND"],
  [["fleet", "kill", FLEET_ID], "UNKNOWN_COMMAND"],
  [["library"], "UNKNOWN_COMMAND"],
  [["secrets", "open"], "UNKNOWN_COMMAND"],
  [["platform", "keys", "set"], "UNKNOWN_COMMAND"],
  [["chat", "--library", FLEET_ID], "UNKNOWN_OPTION"],
  [["chat", "delete", FLEET_ID, "--yes"], "UNKNOWN_OPTION"],
];

describe("removed_group_spellings_are_refused", () => {
  test.each(REMOVED_GROUP_SPELLINGS)("%p is refused as %s", async (argv, code) => {
    const json = await record(["--json", ...argv]);
    expect(json.code).toBe(EXIT_USAGE);
    expect(JSON.parse(json.err).error.code).toBe(code);
    expect(json.calls.filter((call) => call !== "GET /v1/users/me")).toEqual([]);
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

// Sentences that start with the product name but are not commands.
const PROSE = new Set(["agentsfleet cli", "agentsfleet v", "agentsfleet collects"]);
const COMMAND_TEXT = /agentsfleet((?: [a-z][a-z0-9_-]*)+)/g;
const SOURCE_ROOT = path.join(import.meta.dir, "..", "src");

// A validation sentence names its command without the product name:
// "api-keys create requires --name <name>".
const REQUIRES_TEXT = /"([a-z][a-z-]*(?: [a-z][a-z-]*)*) requires (?:--|<)/g;
// A sentence that opens with an article is prose ("the typed form requires
// --provider"), not a command name.
const ARTICLES = new Set(["the", "a", "an", "this", "that", "each", "every"]);

interface PositionalNode { readonly config?: { readonly arguments?: ReadonlyArray<unknown> } }

const positionalsOf = (node: CommandNode): number => ((node as PositionalNode).config?.arguments ?? []).length;
const isLeafNode = (node: CommandNode): boolean => !(node.subcommands ?? []).some((group) => group.commands.length > 0);

// The words after `agentsfleet`, up to the first flag, placeholder or
// interpolation, must name a command, or a leaf followed by no more words
// than it declares positionals. A `requires` sentence must name a leaf exactly.
const unresolved = (text: string): ReadonlyArray<string> => [
  ...[...text.matchAll(COMMAND_TEXT)].flatMap((match) => {
    const words = (match[1] ?? "").trim().split(" ");
    if (PROSE.has(`agentsfleet ${words[0]}`)) return [];
    const walk = walkCommandPath(tree, words);
    const extra = words.length - walk.path.length;
    const fits = extra === 0 || (walk.path.length > 0 && isLeafNode(walk.node) && extra <= positionalsOf(walk.node));
    return fits ? [] : [match[0]];
  }),
  ...[...text.matchAll(REQUIRES_TEXT)].flatMap((match) => {
    const words = (match[1] ?? "").split(" ");
    if (ARTICLES.has(words[0] ?? "")) return [];
    const walk = walkCommandPath(tree, words);
    return walk.path.length === words.length && isLeafNode(walk.node) ? [] : [match[0]];
  }),
];

const withoutComments = (source: string): string =>
  source.split("\n").filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line)).join("\n");

const sourceFiles = async (dir: string): Promise<ReadonlyArray<string>> =>
  Array.fromAsync(new Bun.Glob("**/*.{ts,js}").scan({ cwd: dir, absolute: true }));

describe("cli_hint_strings_name_real_commands", () => {
  test("a stale spelling is detected (positive control)", () => {
    expect(unresolved('const hint = "run agentsfleet kill <fleet_id>";')).toEqual(["agentsfleet kill"]);
    expect(unresolved('const hint = "run agentsfleet fleet archive <fleet_id>";')).toEqual([]);
    expect(unresolved('const hint = "run agentsfleet fleet kill <fleet_id>";')).toEqual(["agentsfleet fleet kill"]);
    expect(unresolved('const hint = "run agentsfleet fleet list extra words";')).toEqual(["agentsfleet fleet list extra words"]);
    expect(unresolved('const hint = "run agentsfleet secrets show github";')).toEqual([]);
    expect(unresolved('detail: "api-key create requires --name <name>"')).toEqual(['"api-key create requires --']);
    expect(unresolved('detail: "api-keys create requires --name <name>"')).toEqual([]);
    expect(unresolved('detail: "the typed form requires --provider"')).toEqual([]);
  });

  test("every command named in CLI source parses", async () => {
    const files = await sourceFiles(SOURCE_ROOT);
    expect(files.length).toBeGreaterThan(50);
    for (const file of files) {
      const found = unresolved(withoutComments(await Bun.file(file).text()));
      expect({ file: path.relative(SOURCE_ROOT, file), found }).toEqual({ file: path.relative(SOURCE_ROOT, file), found: [] });
    }
  });

  test("every command named in rendered output parses", async () => {
    for (const row of COMMAND_PARITY) {
      const run = await record(row.argv);
      expect({ argv: row.argv, found: unresolved(run.out + run.err) }).toEqual({ argv: row.argv, found: [] });
    }
  });
});
