// Parser-level unit tests for the fleet subtree of buildProgram —
// install / list / status / stop / resume / kill / delete / logs / events
// / steer + the secret vault. Sibling file cli-tree.parse.unit.test.js
// covers the top-level + non-fleet tree.

import { test, expect } from "bun:test";

import {
  VALID_ID,
  makeSpyTree,
  dispatch,
  buildSilent,
} from "./helpers-cli-tree.ts";
import { PROVIDER_EXAMPLES, PROVIDER_IDS } from "../src/constants/providers.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../src/constants/custom-endpoint.ts";

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

test("list accepts --workspace-id / --starting-after / --limit", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "list",
    "--workspace-id", VALID_ID,
    "--starting-after", "tok-1",
    "--limit", "50",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.list");
  expect(calls[0]?.frame.parsed.options.workspaceId).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options["workspace-id"]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.startingAfter).toBe("tok-1");
  expect(calls[0]?.frame.parsed.options.limit).toBe(50);
});

test("status dispatches with no positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["status"], handlers);
  expect(calls[0]?.name).toBe("fleet.status");
  expect(calls[0]?.frame.parsed.positionals).toHaveLength(0);
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

test("secret create <name> accepts --data", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "secret", "create", "openai",
    "--data", '{"api_key":"sk-test"}',
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.secret.create");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe("openai");
  expect(calls[0]?.frame.parsed.options.data).toBe('{"api_key":"sk-test"}');
});

test("secret create rejects --force with no dispatch", async () => {
  // Creation claims a free name and the endpoint no longer upserts, so the
  // flag has nothing left to mean. Failing at the parser keeps a script that
  // still passes it from sending a secret body it believes will overwrite.
  const { handlers, calls } = makeSpyTree();
  await expect(dispatch([
    "secret", "create", "openai",
    "--data", '{"api_key":"sk-test"}',
    "--force",
  ], handlers)).rejects.toThrow();
  expect(calls).toHaveLength(0);
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

test("the provider catalogue carries the sentinel without restating its literal", async () => {
  expect(PROVIDER_IDS).toContain(OPENAI_COMPATIBLE_PROVIDER);
  const source = await Bun.file(new URL("../src/constants/providers.ts", import.meta.url)).text();
  expect(source).not.toContain(`"${OPENAI_COMPATIBLE_PROVIDER}"`);
});

test("secret create and update accept a catalogue provider and normalise its case", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "secret", "create", "prod-key",
    "--provider", "Anthropic",
    "--api-key", "sk-named",
    "--model", "claude-opus-5",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet.secret.create");
  expect(calls[0]?.frame.parsed.options.provider).toBe("anthropic");
  await dispatch([
    "secret", "update", "prod-key",
    "--provider", "anthropic",
    "--api-key", "sk-named",
    "--model", "claude-opus-5",
  ], handlers);
  expect(calls[1]?.name).toBe("fleet.secret.update");
  expect(calls[1]?.frame.parsed.options.provider).toBe("anthropic");
});

test("secret create and update reject an unknown provider at parse time (no dispatch)", async () => {
  const { handlers, calls } = makeSpyTree();
  for (const verb of ["create", "update"]) {
    await expect(
      dispatch([
        "secret", verb, "prod-key",
        "--provider", "notaprovider",
        "--api-key", "sk-named",
        "--model", "m",
      ], handlers),
    ).rejects.toThrow("must be one of");
  }
  expect(calls).toHaveLength(0);
});

test("secret create rejects a blank --provider rather than treating it as absent", async () => {
  const { handlers, calls } = makeSpyTree();
  for (const blank of ["", "  "]) {
    await expect(
      dispatch([
        "secret", "create", "prod-key",
        "--provider", blank,
        "--api-key", "sk-named",
        "--model", "m",
      ], handlers),
    ).rejects.toThrow("must be one of");
  }
  expect(calls).toHaveLength(0);
});

test("secret create and update --help name the examples and the true count, not the wall", () => {
  const { handlers } = makeSpyTree();
  const { program } = buildSilent({ handlers });
  const secret = program.commands.find((c) => c.name() === "secret");
  for (const verb of ["create", "update"]) {
    const sub = secret?.commands.find((c) => c.name() === verb);
    const help = sub?.helpInformation() ?? "";
    for (const id of PROVIDER_EXAMPLES) {
      // Boundary-anchored: bare `toContain` cannot catch a dropped short id
      // whose longer sibling contains it (kimi ⊂ kimi-intl).
      expect(help).toMatch(new RegExp(`(?<![\\w-])${id}(?![\\w-])`));
    }
    // Commander wraps descriptions at column width, so assert on a
    // whitespace-normalized view — the count may straddle a line break.
    const flat = help.replace(/\s+/g, " ");
    // The count is live, so a catalogue change cannot leave help lying about
    // the size of the accepted set…
    expect(flat).toContain(`${PROVIDER_IDS.length} accepted`);
    // …and the full wall stays out of help; the unknown-value error carries it.
    expect(flat).not.toContain(PROVIDER_IDS.slice(0, 8).join(", "));
  }
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
