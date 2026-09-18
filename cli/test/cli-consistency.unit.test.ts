// One spelling per concept, and one shape per list.
//
// Each case here corresponds to a place the surface contradicted itself: the
// library identifier read three ways, the vault list printed a shape no other
// list printed, and a next-page hint named a command that does not exist.

import { describe, test, expect } from "bun:test";

import { LIBRARY_ID_PLACEHOLDER } from "../src/constants/cli-flags.ts";
import { USAGE_INSTALL } from "../src/commands/fleet_install_source.ts";
import { VALID_ID, buildSilent, dispatch, makeSpyTree } from "./helpers-cli-tree.ts";

const buildTestProgram = () => buildSilent({ handlers: makeSpyTree().handlers });

/** Every help string the program can print, flattened. */
const allHelpText = (): string => {
  const { program } = buildTestProgram();
  const walk = (cmd: { commands?: unknown[]; helpInformation: () => string }): string => {
    const own = cmd.helpInformation();
    const kids = (cmd.commands ?? []) as Array<typeof cmd>;
    return [own, ...kids.map(walk)].join("\n");
  };
  return walk(program as unknown as Parameters<typeof walk>[0]);
};

describe("library identifier — one spelling everywhere", () => {
  test("the placeholder is the single source the other sites read", () => {
    expect(LIBRARY_ID_PLACEHOLDER).toBe("<library_id>");
    expect(USAGE_INSTALL).toContain(LIBRARY_ID_PLACEHOLDER);
  });

  test("no help text spells the library identifier any other way", () => {
    const help = allHelpText();
    // `--library <id>` and `<library>` were the two competing spellings.
    expect(help).not.toContain("--library <id>");
    expect(help).not.toMatch(/--library <library>(?!_)/);
    expect(help).toContain(`--library ${LIBRARY_ID_PLACEHOLDER}`);
  });
});

describe("command surface — every hint names a command that exists", () => {
  test("the registered top-level command names include the ones hints point at", () => {
    const { program } = buildTestProgram();
    const names = (program.commands as ReadonlyArray<{ name: () => string }>).map((c) => c.name());
    // The paginated-list hint used to say `agentsfleet fleet list`, which is
    // not a command: `fleet` holds `update` alone.
    expect(names).toContain("list");
    expect(names).toContain("approvals");
    expect(names).toContain("library");
    const fleet = (program.commands as ReadonlyArray<{ name: () => string; commands: ReadonlyArray<{ name: () => string }> }>)
      .find((c) => c.name() === "fleet");
    expect(fleet?.commands.map((c) => c.name())).not.toContain("list");
  });

  test("`library` carries an `add` subcommand and still runs bare", () => {
    const { program } = buildTestProgram();
    const library = (program.commands as ReadonlyArray<{ name: () => string; commands: ReadonlyArray<{ name: () => string }> }>)
      .find((c) => c.name() === "library");
    expect(library?.commands.map((c) => c.name())).toContain("add");
  });

  test("`approvals` carries list, show, approve, and deny", () => {
    const { program } = buildTestProgram();
    const approvals = (program.commands as ReadonlyArray<{ name: () => string; commands: ReadonlyArray<{ name: () => string }> }>)
      .find((c) => c.name() === "approvals");
    const subs = approvals?.commands.map((c) => c.name()) ?? [];
    for (const verb of ["list", "show", "approve", "deny"]) {
      expect(subs).toContain(verb);
    }
  });
});

describe("logout — the help states only what the daemon does", () => {
  test("it does not claim to revoke every session on the account", () => {
    const help = allHelpText();
    // `/v1/auth/sessions/all` aborts in-flight logins and, in its own words,
    // "Does NOT revoke already-minted JWTs". Other machines keep working.
    expect(help).not.toContain("revoke every active session on this account");
    expect(help).toContain("other machines stay signed in");
  });
});

describe("new commands route to a bound handler", () => {
  // Every handler is built through `wrapE` / `wrapEFn` in handlers-bind.ts,
  // which is the single seam `withCommandInstrumentation` is applied at. So a
  // command that reaches its handler is instrumented by construction, and the
  // thing that can actually go wrong is a command registered in the tree with
  // nothing bound behind it. That is what these assert.
  const cases: ReadonlyArray<{ argv: string[]; handler: string }> = [
    { argv: ["library", "add", "--github", "owner/repo"], handler: "fleet.library.add" },
    { argv: ["approvals", "list"], handler: "approvals.list" },
    { argv: ["approvals", "show", VALID_ID], handler: "approvals.show" },
    { argv: ["approvals", "approve", VALID_ID], handler: "approvals.approve" },
    { argv: ["approvals", "deny", VALID_ID], handler: "approvals.deny" },
  ];

  for (const { argv, handler } of cases) {
    test(`\`${argv.join(" ")}\` reaches ${handler}`, async () => {
      const { handlers, calls } = makeSpyTree();
      await dispatch(argv, handlers);
      expect(calls.map((c) => c.name)).toContain(handler);
    });
  }

  test("bare `library` still reaches the gallery handler, not the add handler", async () => {
    const { handlers, calls } = makeSpyTree();
    await dispatch(["library"], handlers);
    const names = calls.map((c) => c.name);
    expect(names).toContain("fleet.library");
    expect(names).not.toContain("fleet.library.add");
  });
});
