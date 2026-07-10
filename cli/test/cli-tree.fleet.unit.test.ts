// Parser-level unit tests for the fleet subtree of buildProgram —
// install / list / status / stop / resume / kill / delete / logs / events
// / steer + the secret vault. Sibling file cli-tree.parse.unit.test.js
// covers the top-level + non-fleet tree.

import { test, expect } from "bun:test";

import {
  VALID_ID,
  makeSpyTree,
  dispatch,
} from "./helpers-cli-tree.ts";

test("install accepts --library <id> and --name <name>", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(
    ["install", "--library", "github-pr-reviewer", "--name", "pr-frontend"],
    handlers,
  );
  expect(calls[0]?.name).toBe("fleet.install");
  expect(calls[0]?.frame.parsed.options.library).toBe("github-pr-reviewer");
  expect(calls[0]?.frame.parsed.options.name).toBe("pr-frontend");
});

test("library dispatches with no options", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["library"], handlers);
  expect(calls[0]?.name).toBe("fleet.library");
  expect(calls[0]?.frame.parsed.positionals).toHaveLength(0);
});

test("fleet update <id> accepts --from <path>", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["fleet", "update", VALID_ID, "--from", "/tmp/skill"], handlers);
  expect(calls[0]?.name).toBe("fleet.update");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.from).toBe("/tmp/skill");
});

test("list accepts --workspace-id / --cursor / --limit", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "list",
    "--workspace-id", VALID_ID,
    "--cursor", "tok-1",
    "--limit", "50",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.list");
  expect(calls[0]?.frame.parsed.options.workspaceId).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options["workspace-id"]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.cursor).toBe("tok-1");
  expect(calls[0]?.frame.parsed.options.limit).toBe(50);
});

test("status [fleet_id] dispatches with no positional (workspace-wide)", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["status"], handlers);
  expect(calls[0]?.name).toBe("fleet.status");
  expect(calls[0]?.frame.parsed.positionals).toHaveLength(0);
});

test("status <fleet_id> dispatches with positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["status", VALID_ID], handlers);
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

test("stop / resume / kill / delete each dispatch with required positional", async () => {
  for (const cmd of ["stop", "resume", "kill", "delete"]) {
    const { handlers, calls } = makeSpyTree();
    await dispatch([cmd, VALID_ID], handlers);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.name).toBe(`fleet.${cmd}`);
    expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  }
});

test("logs accepts --fleet / --limit / --cursor", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "logs",
    "--fleet", VALID_ID,
    "--limit", "100",
    "--cursor", "next-tok",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.logs");
  expect(calls[0]?.frame.parsed.options.fleet).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.limit).toBe(100);
});

test("events <id> accepts --actor / --since / --cursor / --limit", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "events", VALID_ID,
    "--actor", "human:*",
    "--since", "2h",
    "--cursor", "next",
    "--limit", "200",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.events");
  expect(calls[0]?.frame.parsed.options.actor).toBe("human:*");
  expect(calls[0]?.frame.parsed.options.since).toBe("2h");
  expect(calls[0]?.frame.parsed.options.limit).toBe(200);
});

test("steer <id> <message> dispatches with two positionals", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["steer", VALID_ID, "hello there"], handlers);
  expect(calls[0]?.name).toBe("fleet.steer");
  expect(calls[0]?.frame.parsed.positionals).toEqual([VALID_ID, "hello there"]);
});

test("steer <id> --tty dispatches without a message", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["steer", VALID_ID, "--tty"], handlers);
  expect(calls[0]?.name).toBe("fleet.steer");
  expect(calls[0]?.frame.parsed.positionals).toEqual([VALID_ID]);
  expect(calls[0]?.frame.parsed.options.tty).toBe(true);
});

test("secret create <name> accepts --data / --force", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "secret", "create", "openai",
    "--data", '{"api_key":"sk-test"}',
    "--force",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.secret.create");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe("openai");
  expect(calls[0]?.frame.parsed.options.data).toBe('{"api_key":"sk-test"}');
  expect(calls[0]?.frame.parsed.options.force).toBe(true);
});

test("secret add is rejected with no dispatch", async () => {
  const { handlers, calls } = makeSpyTree();
  await expect(dispatch(["secret", "add", "openai"], handlers)).rejects.toThrow();
  expect(calls).toHaveLength(0);
});

test("secret create <name> accepts the typed custom-endpoint flags", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "secret", "create", "vllm",
    "--provider", "openai-compatible",
    "--base-url", "https://vllm.corp/v1",
    "--api-key", "sk-custom",
    "--model", "qwen2.5",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.secret.create");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe("vllm");
  expect(calls[0]?.frame.parsed.options.provider).toBe("openai-compatible");
  // commander stores hyphenated flags under their camelCase key.
  expect(calls[0]?.frame.parsed.options.baseUrl).toBe("https://vllm.corp/v1");
  expect(calls[0]?.frame.parsed.options.apiKey).toBe("sk-custom");
  expect(calls[0]?.frame.parsed.options.model).toBe("qwen2.5");
});

test("secret create rejects a non-https --base-url at parse time (no dispatch)", async () => {
  const { handlers, calls } = makeSpyTree();
  await expect(
    dispatch([
      "secret", "create", "vllm",
      "--provider", "openai-compatible",
      "--base-url", "http://vllm.corp/v1",
      "--api-key", "sk-custom",
    ], handlers),
  ).rejects.toThrow(/https/i);
  // The validator threw during parse — the handler never ran.
  expect(calls).toHaveLength(0);
});

test("secret show / list / delete each dispatch with the right shape", async () => {
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["secret", "show", "openai"], handlers);
    expect(calls[0]?.name).toBe("fleet.secret.show");
    expect(calls[0]?.frame.parsed.positionals[0]).toBe("openai");
  }
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["secret", "list"], handlers);
    expect(calls[0]?.name).toBe("fleet.secret.list");
  }
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["secret", "delete", "openai"], handlers);
    expect(calls[0]?.name).toBe("fleet.secret.delete");
    expect(calls[0]?.frame.parsed.positionals[0]).toBe("openai");
  }
});
