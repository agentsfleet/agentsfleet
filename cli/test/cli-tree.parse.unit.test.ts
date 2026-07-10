// Parser-level unit tests for buildProgram (top-level + non-fleet tree).
// Drives commander directly with a no-op handlers tree so every actionFor()
// closure fires for its argv. Companion file cli-tree.fleet.unit.test.js
// covers the fleet / secret subtree.

import { test, expect } from "bun:test";
import { CommanderError, type Help } from "commander";

import {
  VALID_ID,
  makeSpyTree,
  buildSilent,
  dispatch,
} from "./helpers-cli-tree.ts";
import { buildProgram } from "../src/program/cli-tree.ts";
import type { Handlers } from "../src/program/cli-tree-types.ts";

// ── User commands ───────────────────────────────────────────────────────

test("login dispatches and propagates --token", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["login", "--token", "pat_abc123"], handlers);
  expect(calls).toHaveLength(1);
  expect(calls[0]?.name).toBe("login");
  expect(calls[0]?.frame.parsed.options.token).toBe("pat_abc123");
});

test("logout dispatches with no options", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["logout"], handlers);
  expect(calls).toHaveLength(1);
  expect(calls[0]?.name).toBe("logout");
});

test("doctor dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["doctor"], handlers);
  expect(calls).toHaveLength(1);
  expect(calls[0]?.name).toBe("doctor");
});

test("auth status dispatches the nested status action", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["auth", "status"], handlers);
  expect(calls).toHaveLength(1);
  expect(calls[0]?.name).toBe("auth.status");
});

// ── Workspace tree ──────────────────────────────────────────────────────

test("workspace create [name] captures optional positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "create", "my-ws"], handlers);
  expect(calls[0]?.name).toBe("workspace.create");
  expect(calls[0]?.frame.parsed.positionals).toEqual(["my-ws"]);
});

test("workspace add is rejected with no dispatch", async () => {
  const { handlers, calls } = makeSpyTree();
  await expect(dispatch(["workspace", "add", "my-ws"], handlers)).rejects.toThrow();
  expect(calls).toHaveLength(0);
});

test("workspace list dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "list"], handlers);
  expect(calls[0]?.name).toBe("workspace.list");
});

test("workspace use <id> captures required positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "use", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("workspace.use");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

test("workspace show [id] accepts positional OR --workspace-id flag", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "show", "--workspace-id", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("workspace.show");
  expect(calls[0]?.frame.parsed.options.workspaceId).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options["workspace-id"]).toBe(VALID_ID);
});

test("workspace secrets dispatches (auth-only redirect surface)", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "secrets"], handlers);
  expect(calls[0]?.name).toBe("workspace.secrets");
});

test("workspace delete <id> captures required positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "delete", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("workspace.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

// ── Fleet-key tree ────────────────────────────────────────────────────────

test("fleet-key create accepts --workspace / --fleet / --name / --description", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "fleet-key", "create",
    "--workspace", VALID_ID,
    "--fleet",    VALID_ID,
    "--name",      "scout",
    "--description", "for scouting",
  ], handlers);
  expect(calls[0]?.name).toBe("fleet-key.create");
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.fleet).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.name).toBe("scout");
  expect(calls[0]?.frame.parsed.options.description).toBe("for scouting");
});

test("fleet-key add is rejected with no dispatch", async () => {
  const { handlers, calls } = makeSpyTree();
  await expect(dispatch(["fleet-key", "add"], handlers)).rejects.toThrow();
  expect(calls).toHaveLength(0);
});

test("fleet-key list with --workspace dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["fleet-key", "list", "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("fleet-key.list");
});

test("fleet-key delete <id> with --workspace captures both", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["fleet-key", "delete", VALID_ID, "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("fleet-key.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
});

// ── API-key tree ─────────────────────────────────────────────────────────

test("api-key create accepts --name / --description", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "api-key", "create",
    "--name", "ci-runner",
    "--description", "build automation",
  ], handlers);
  expect(calls[0]?.name).toBe("api-key.create");
  expect(calls[0]?.frame.parsed.options.name).toBe("ci-runner");
  expect(calls[0]?.frame.parsed.options.description).toBe("build automation");
});

test("api-key list accepts pagination and sort flags", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "api-key", "list",
    "--page", "2",
    "--page-size", "50",
    "--sort", "key_name",
  ], handlers);
  expect(calls[0]?.name).toBe("api-key.list");
  expect(calls[0]?.frame.parsed.options.page).toBe(2);
  expect(calls[0]?.frame.parsed.options.pageSize).toBe(50);
  expect(calls[0]?.frame.parsed.options["page-size"]).toBe(50);
  expect(calls[0]?.frame.parsed.options.sort).toBe("key_name");
});

test("api-key revoke/delete capture the key id", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["api-key", "revoke", VALID_ID], handlers);
  await dispatch(["api-key", "delete", VALID_ID], handlers);
  expect(calls.map((c) => c.name)).toEqual(["api-key.revoke", "api-key.delete"]);
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[1]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

// ── Connector tree ───────────────────────────────────────────────────────

test("connector list dispatches with --workspace", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["connector", "list", "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("connector.list");
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
});

test("connector status captures provider and --workspace", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["connector", "status", "slack", "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("connector.status");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe("slack");
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
});

// ── Grant tree ──────────────────────────────────────────────────────────

test("grant list dispatches with --fleet option", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["grant", "list", "--fleet", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("grant.list");
  expect(calls[0]?.frame.parsed.options.fleet).toBe(VALID_ID);
});

test("grant delete <id> with --fleet captures both", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["grant", "delete", VALID_ID, "--fleet", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("grant.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

// ── Tenant provider tree ────────────────────────────────────────────────

test("tenant provider show dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["tenant", "provider", "show"], handlers);
  expect(calls[0]?.name).toBe("tenant.provider.show");
});

test("tenant provider create accepts --secret / --model", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "tenant", "provider", "create",
    "--secret", "openai-prod",
    "--model",      "gpt-4o",
  ], handlers);
  expect(calls[0]?.name).toBe("tenant.provider.create");
  expect(calls[0]?.frame.parsed.options.secret).toBe("openai-prod");
  expect(calls[0]?.frame.parsed.options.model).toBe("gpt-4o");
});

test("tenant provider add is rejected with no dispatch", async () => {
  const { handlers, calls } = makeSpyTree();
  await expect(dispatch(["tenant", "provider", "add"], handlers)).rejects.toThrow();
  expect(calls).toHaveLength(0);
});

test("tenant provider delete dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["tenant", "provider", "delete"], handlers);
  expect(calls[0]?.name).toBe("tenant.provider.delete");
});

// ── Billing tree ────────────────────────────────────────────────────────

test("billing show accepts --limit / --cursor", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["billing", "show", "--limit", "25", "--cursor", "abc"], handlers);
  expect(calls[0]?.name).toBe("billing.show");
  expect(calls[0]?.frame.parsed.options.limit).toBe(25);
  expect(calls[0]?.frame.parsed.options.cursor).toBe("abc");
});

// ── Global options propagate via optsWithGlobals() ──────────────────────

test("--api / --json globals are visible on the leaf frame", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "--api", "https://api.example.test",
    "--json",
    "doctor",
  ], handlers);
  expect(calls[0]?.name).toBe("doctor");
  expect(calls[0]?.frame.parsed.options.api).toBe("https://api.example.test");
  expect(calls[0]?.frame.parsed.options.json).toBe(true);
});

test("--no-input / --no-open normalise to opts.input/open === false", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["--no-input", "--no-open", "login"], handlers);
  expect(calls[0]?.frame.parsed.options.input).toBe(false);
  expect(calls[0]?.frame.parsed.options.open).toBe(false);
});

// ── runHandler edge: missing handler raises with exitCode=2 ─────────────

test("runHandler raises and sets state.exitCode=2 when a leaf handler is not a function", async () => {
  const handlers = { login: undefined } as unknown as Handlers;
  const { program, state } = buildSilent({ handlers });
  let captured: unknown = null;
  try {
    await program.parseAsync(["login"], { from: "user" });
  } catch (err) {
    captured = err;
  }
  expect(captured).not.toBeNull();
  expect(captured).toBeInstanceOf(Error);
  if (captured instanceof Error) {
    expect(captured.message).toMatch(/no handler wired for command: login/);
  }
  expect(state.exitCode).toBe(2);
});

// ── Validator rejection path: parseIntOption raises InvalidArgumentError ─

test("--limit 0 on billing show is rejected by parseIntOption (commander InvalidArgumentError)", async () => {
  const { handlers, calls } = makeSpyTree();
  const { program } = buildSilent({ handlers });
  let captured: unknown = null;
  try {
    await program.parseAsync(["billing", "show", "--limit", "0"], { from: "user" });
  } catch (err) {
    captured = err;
  }
  expect(captured).toBeInstanceOf(CommanderError);
  if (captured instanceof CommanderError) {
    expect(captured.code).toBe("commander.invalidArgument");
  }
  expect(calls).toHaveLength(0);
});

// ── helpFactory injection point exists at construction ─────────────────

test("helpFactory is deferred — not invoked at construction, fires when help renders", async () => {
  let factoryCalls = 0;
  const { handlers } = makeSpyTree();
  const state = { exitCode: 0 };
  const program = buildProgram({
    handlers,
    version: "0.0.0-test",
    state,
    helpFactory: () => {
      factoryCalls += 1;
      return {
        formatHelp: () => "",
        visibleCommands: () => [],
        visibleOptions: () => [],
      } as unknown as Help;
    },
  });
  // Construction alone must not invoke the factory — cli.ts needs to
  // wire ctx-aware help renderers around it after buildProgram returns.
  expect(factoryCalls).toBe(0);

  program.exitOverride();
  program.configureOutput({ writeOut: () => {}, writeErr: () => {} });
  try {
    await program.parseAsync(["--help"], { from: "user" });
  } catch {
    // commander throws CommanderError(0, "commander.helpDisplayed") after
    // rendering help; that's the expected control-flow.
  }
  expect(factoryCalls).toBeGreaterThan(0);
});

// ── Default help factory closure fires when no helpFactory is injected ───

test("default createHelp (() => new FleetHelp()) renders --help when no factory is supplied", async () => {
  const { handlers } = makeSpyTree();
  const state = { exitCode: 0 };
  // No helpFactory → buildProgram installs the default `() => new FleetHelp()`
  // closure. Rendering --help invokes it, covering that arrow.
  const program = buildProgram({ handlers, version: "0.0.0-test", state });
  program.exitOverride();
  let rendered = "";
  program.configureOutput({
    writeOut: (s) => {
      rendered += s;
    },
    writeErr: () => {},
  });
  try {
    await program.parseAsync(["--help"], { from: "user" });
  } catch {
    // commander throws CommanderError(0, "commander.helpDisplayed") post-render.
  }
  expect(rendered).toContain("agentsfleet");
  expect(rendered).toContain("Environment variables:");
});
