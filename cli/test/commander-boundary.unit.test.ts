// In-process cover for the commander boundary. The acceptance sweep proves
// the same behaviour end to end, but it spawns a subprocess, so none of it
// reaches the coverage floor this package enforces — these exercise the same
// paths inside the test process.

import { describe, expect, test } from "bun:test";
import { Command } from "commander";

import { runCli } from "../src/cli.ts";
import {
  applyOutputToTree,
  captureRejection,
  commandPath,
  commandUsageLine,
  exitFromCommanderError,
  rejectionCodeFor,
  renderRejection,
  routeGroupHelpToStdout,
} from "../src/lib/commander-boundary.ts";
import { REJECTION_CODE } from "../src/constants/rejection.ts";
import { bufferStream } from "./helpers-cli-state.ts";

function tree(): Command {
  const program = new Command();
  program.name("agentsfleet");
  const group = program.command("widget").description("widgets");
  group.command("show <widget_id>").description("show one").action(() => {});
  program.command("ping").description("ping").action(() => {});
  return program;
}

function find(program: Command, ...names: string[]): Command {
  let cmd: Command = program;
  for (const name of names) {
    const next = cmd.commands.find((c) => c.name() === name);
    if (!next) throw new Error(`no subcommand ${name}`);
    cmd = next;
  }
  return cmd;
}

describe("commandPath / commandUsageLine", () => {
  test("a leaf names its full path and its declared arguments", () => {
    const cmd = find(tree(), "widget", "show");
    expect(commandPath(cmd)).toBe("agentsfleet widget show");
    expect(commandUsageLine(cmd)).toBe("usage: agentsfleet widget show [options] <widget_id>");
  });

  test("a group points at its command list instead of a usage string", () => {
    expect(commandUsageLine(find(tree(), "widget")))
      .toBe("run `agentsfleet widget --help` for the command list");
  });

  test("the root points at its own command list", () => {
    expect(commandUsageLine(tree()))
      .toBe("run `agentsfleet --help` for the command list");
  });
});

describe("captureRejection", () => {
  const cmd = () => find(tree(), "widget", "show");

  test("strips commander's error stem", () => {
    const captured = captureRejection("error: missing required argument 'widget_id'", cmd(), null);
    expect(captured?.detail).toBe("missing required argument 'widget_id'");
  });

  test("keeps text that carries no stem", () => {
    expect(captureRejection("something else", cmd(), null)?.detail).toBe("something else");
  });

  test("drops the help hint and blank writes", () => {
    expect(captureRejection("(use --help for usage)", cmd(), null)).toBeNull();
    expect(captureRejection("   ", cmd(), null)).toBeNull();
  });

  test("keeps the first rejection when commander writes twice", () => {
    const first = captureRejection("error: first", cmd(), null);
    expect(captureRejection("error: second", cmd(), first)).toBe(first);
  });
});

describe("renderRejection", () => {
  const pending = { detail: "missing required argument 'widget_id'", usageLine: "usage: x" };

  test("human mode joins the detail and the suggestion", () => {
    expect(renderRejection(pending, REJECTION_CODE.missingArgument, false))
      .toBe("missing required argument 'widget_id'\n  Suggestion: usage: x");
  });

  test("json mode emits the machine envelope", () => {
    const parsed = JSON.parse(renderRejection(pending, REJECTION_CODE.missingArgument, true)) as {
      error: { code: string; message: string };
    };
    expect(parsed.error.code).toBe("MISSING_ARGUMENT");
    expect(parsed.error.message).toBe(pending.detail);
  });

  test("json mode carries a null code for a commander code with no mapping", () => {
    const parsed = JSON.parse(renderRejection(pending, null, true)) as { error: { code: null } };
    expect(parsed.error.code).toBeNull();
  });
});

describe("rejectionCodeFor", () => {
  test("maps a known commander code", () => {
    expect(rejectionCodeFor("commander.missingArgument")).toBe(REJECTION_CODE.missingArgument);
  });

  test("returns null for anything else", () => {
    expect(rejectionCodeFor("commander.helpDisplayed")).toBeNull();
  });
});

describe("exitFromCommanderError", () => {
  const err = (code: string, exitCode = 1) =>
    ({ code, exitCode }) as unknown as Parameters<typeof exitFromCommanderError>[0];

  test("help exits 0", () => {
    expect(exitFromCommanderError(err("commander.help"), { exitCode: 0 }, 4)).toBe(0);
    expect(exitFromCommanderError(err("commander.helpDisplayed"), { exitCode: 0 }, 4)).toBe(0);
  });

  test("a handler-set exit code wins over the usage mapping", () => {
    expect(exitFromCommanderError(err("commander.missingArgument"), { exitCode: 7 }, 4)).toBe(7);
  });

  test("a usage code takes the caller's usage exit", () => {
    expect(exitFromCommanderError(err("commander.missingArgument"), { exitCode: 0 }, 4)).toBe(4);
  });

  test("a non-usage code keeps commander's own exit", () => {
    expect(exitFromCommanderError(err("commander.version", 3), { exitCode: 0 }, 4)).toBe(3);
  });
});

describe("routeGroupHelpToStdout", () => {
  test("a group's error-context help lands on stdout", () => {
    const out = bufferStream();
    const err = bufferStream();
    const program = tree();
    applyOutputToTree(program, out.stream, err.stream, () => {});
    routeGroupHelpToStdout(program);
    const group = find(program, "widget");
    expect(() => group.help({ error: true })).toThrow();
    expect(out.read()).toMatch(/Usage: agentsfleet widget/);
    expect(err.read()).toBe("");
  });

  test("a leaf keeps commander's own routing", () => {
    const out = bufferStream();
    const err = bufferStream();
    const program = tree();
    applyOutputToTree(program, out.stream, err.stream, () => {});
    routeGroupHelpToStdout(program);
    const leaf = find(program, "ping");
    expect(() => leaf.help({ error: true })).toThrow();
    expect(err.read()).toMatch(/Usage: agentsfleet ping/);
  });
});

describe("applyOutputToTree", () => {
  test("routes commander's own stderr writes to the injected stream", () => {
    const out = bufferStream();
    const err = bufferStream();
    const program = tree();
    applyOutputToTree(program, out.stream, err.stream, () => {});
    // `{ error: true }` is the path a group node's bare invocation takes
    // inside commander; it must reach the injected stderr, not the real one.
    program.outputHelp({ error: true });
    expect(err.read()).toMatch(/Usage: agentsfleet/);
  });

  test("hands rejection text to the recorder instead of writing it", () => {
    const out = bufferStream();
    const err = bufferStream();
    const program = tree();
    const seen: string[] = [];
    applyOutputToTree(program, out.stream, err.stream, (text) => seen.push(text));
    find(program, "widget").configureOutput().outputError?.("error: boom", () => {});
    expect(seen).toEqual(["error: boom"]);
    expect(err.read()).toBe("");
  });
});

describe("a bare group node through runCli", () => {
  test("prints help on stdout and exits 0", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["workspace"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(out.read()).toMatch(/Usage: agentsfleet workspace/);
    expect(err.read()).toBe("");
  });
});
