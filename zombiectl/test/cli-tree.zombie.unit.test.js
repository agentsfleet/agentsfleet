// Parser-level unit tests for the zombie subtree of buildProgram —
// install / list / status / stop / resume / kill / delete / logs / events
// / steer + the credential vault. Sibling file cli-tree.parse.unit.test.js
// covers the top-level + non-zombie tree.

import { test, expect } from "bun:test";

import {
  VALID_ID,
  makeSpyTree,
  dispatch,
} from "./helpers-cli-tree.ts";

test("install accepts --from <path>", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["install", "--from", "/tmp/skill"], handlers);
  expect(calls[0].name).toBe("zombie.install");
  expect(calls[0].frame.parsed.options.from).toBe("/tmp/skill");
});

test("zombie update <id> accepts --from <path>", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["zombie", "update", VALID_ID, "--from", "/tmp/skill"], handlers);
  expect(calls[0].name).toBe("zombie.update");
  expect(calls[0].frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[0].frame.parsed.options.from).toBe("/tmp/skill");
});

test("list accepts --workspace-id / --cursor / --limit", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "list",
    "--workspace-id", VALID_ID,
    "--cursor", "tok-1",
    "--limit", "50",
  ], handlers);
  expect(calls[0].name).toBe("zombie.list");
  expect(calls[0].frame.parsed.options.workspaceId).toBe(VALID_ID);
  expect(calls[0].frame.parsed.options["workspace-id"]).toBe(VALID_ID);
  expect(calls[0].frame.parsed.options.cursor).toBe("tok-1");
  expect(calls[0].frame.parsed.options.limit).toBe(50);
});

test("status [zombie_id] dispatches with no positional (workspace-wide)", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["status"], handlers);
  expect(calls[0].name).toBe("zombie.status");
  expect(calls[0].frame.parsed.positionals).toHaveLength(0);
});

test("status <zombie_id> dispatches with positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["status", VALID_ID], handlers);
  expect(calls[0].frame.parsed.positionals[0]).toBe(VALID_ID);
});

test("stop / resume / kill / delete each dispatch with required positional", async () => {
  for (const cmd of ["stop", "resume", "kill", "delete"]) {
    const { handlers, calls } = makeSpyTree();
    await dispatch([cmd, VALID_ID], handlers);
    expect(calls).toHaveLength(1);
    expect(calls[0].name).toBe(`zombie.${cmd}`);
    expect(calls[0].frame.parsed.positionals[0]).toBe(VALID_ID);
  }
});

test("logs accepts --zombie / --limit / --cursor", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "logs",
    "--zombie", VALID_ID,
    "--limit", "100",
    "--cursor", "next-tok",
  ], handlers);
  expect(calls[0].name).toBe("zombie.logs");
  expect(calls[0].frame.parsed.options.zombie).toBe(VALID_ID);
  expect(calls[0].frame.parsed.options.limit).toBe(100);
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
  expect(calls[0].name).toBe("zombie.events");
  expect(calls[0].frame.parsed.options.actor).toBe("human:*");
  expect(calls[0].frame.parsed.options.since).toBe("2h");
  expect(calls[0].frame.parsed.options.limit).toBe(200);
});

test("steer <id> <message> dispatches with two positionals", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["steer", VALID_ID, "hello there"], handlers);
  expect(calls[0].name).toBe("zombie.steer");
  expect(calls[0].frame.parsed.positionals).toEqual([VALID_ID, "hello there"]);
});

test("credential add <name> accepts --data / --force", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "credential", "add", "openai",
    "--data", '{"api_key":"sk-test"}',
    "--force",
  ], handlers);
  expect(calls[0].name).toBe("zombie.credential.add");
  expect(calls[0].frame.parsed.positionals[0]).toBe("openai");
  expect(calls[0].frame.parsed.options.data).toBe('{"api_key":"sk-test"}');
  expect(calls[0].frame.parsed.options.force).toBe(true);
});

test("credential show / list / delete each dispatch with the right shape", async () => {
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["credential", "show", "openai"], handlers);
    expect(calls[0].name).toBe("zombie.credential.show");
    expect(calls[0].frame.parsed.positionals[0]).toBe("openai");
  }
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["credential", "list"], handlers);
    expect(calls[0].name).toBe("zombie.credential.list");
  }
  {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["credential", "delete", "openai"], handlers);
    expect(calls[0].name).toBe("zombie.credential.delete");
    expect(calls[0].frame.parsed.positionals[0]).toBe("openai");
  }
});
