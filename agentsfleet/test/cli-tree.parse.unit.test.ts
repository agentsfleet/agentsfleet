// Parser-level unit tests for buildProgram (top-level + non-zombie tree).
// Drives commander directly with a no-op handlers tree so every actionFor()
// closure fires for its argv. Companion file cli-tree.zombie.unit.test.js
// covers the zombie / credential subtree.

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

test("workspace add [name] captures optional positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "add", "my-ws"], handlers);
  expect(calls[0]?.name).toBe("workspace.add");
  expect(calls[0]?.frame.parsed.positionals).toEqual(["my-ws"]);
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

test("workspace credentials dispatches (auth-only redirect surface)", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "credentials"], handlers);
  expect(calls[0]?.name).toBe("workspace.credentials");
});

test("workspace delete <id> captures required positional", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["workspace", "delete", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("workspace.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

// ── Agent tree ──────────────────────────────────────────────────────────

test("agent add accepts --workspace / --zombie / --name / --description", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "agent", "add",
    "--workspace", VALID_ID,
    "--zombie",    VALID_ID,
    "--name",      "scout",
    "--description", "for scouting",
  ], handlers);
  expect(calls[0]?.name).toBe("agent.add");
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.zombie).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.name).toBe("scout");
  expect(calls[0]?.frame.parsed.options.description).toBe("for scouting");
});

test("agent list with --workspace dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["agent", "list", "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("agent.list");
});

test("agent delete <id> with --workspace captures both", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["agent", "delete", VALID_ID, "--workspace", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("agent.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
  expect(calls[0]?.frame.parsed.options.workspace).toBe(VALID_ID);
});

// ── Grant tree ──────────────────────────────────────────────────────────

test("grant list dispatches with --zombie option", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["grant", "list", "--zombie", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("grant.list");
  expect(calls[0]?.frame.parsed.options.zombie).toBe(VALID_ID);
});

test("grant delete <id> with --zombie captures both", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["grant", "delete", VALID_ID, "--zombie", VALID_ID], handlers);
  expect(calls[0]?.name).toBe("grant.delete");
  expect(calls[0]?.frame.parsed.positionals[0]).toBe(VALID_ID);
});

// ── Tenant provider tree ────────────────────────────────────────────────

test("tenant provider show dispatches", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch(["tenant", "provider", "show"], handlers);
  expect(calls[0]?.name).toBe("tenant.provider.show");
});

test("tenant provider add accepts --credential / --model", async () => {
  const { handlers, calls } = makeSpyTree();
  await dispatch([
    "tenant", "provider", "add",
    "--credential", "openai-prod",
    "--model",      "gpt-4o",
  ], handlers);
  expect(calls[0]?.name).toBe("tenant.provider.add");
  expect(calls[0]?.frame.parsed.options.credential).toBe("openai-prod");
  expect(calls[0]?.frame.parsed.options.model).toBe("gpt-4o");
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

test("default createHelp (() => new ZombieHelp()) renders --help when no factory is supplied", async () => {
  const { handlers } = makeSpyTree();
  const state = { exitCode: 0 };
  // No helpFactory → buildProgram installs the default `() => new ZombieHelp()`
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
  expect(rendered).toContain("Subcommands:");
});
