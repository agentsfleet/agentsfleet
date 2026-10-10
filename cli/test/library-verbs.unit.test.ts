// `library` spoke the same private dialect `schedule` did: add where every
// other collection says create, and remove where the rest says delete.
//
// `remove` never shipped — it arrives and is renamed inside one milestone — but
// `add` did, in 0.49.0, and no alias is kept for it. A compatibility verb is
// what the rules forbid at this version, so a retired spelling answers as the
// unknown subcommand it now is.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { WS_ID, authedScope, jsonResponse, withMockApi, type MockRoutes } from "./helpers-fleet-install.ts";

const LIBRARY = "library";

const RENAMED = [
  ["add", "create"],
  ["remove", "delete"],
] as const;

const help = async (argv: ReadonlyArray<string>) => {
  const out = bufferStream();
  const err = bufferStream();
  const code = await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({}) });
  return { code, stdout: out.read(), stderr: err.read() };
};

describe("the library verbs match the rest of the surface", () => {
  test.each(RENAMED.map(([, to]) => to))("library %s is a real subcommand", async (verb) => {
    const { code, stdout } = await help(["library", verb, "--help"]);
    expect(code).toBe(0);
    expect(stdout).toContain(verb);
  });

  test.each(RENAMED.map(([from]) => from))(
    "the retired spelling %s is refused, pointing at the group's list",
    async (verb) => {
      const { code, stderr } = await help(["library", verb]);
      expect(code).not.toBe(0);
      expect(stderr).toContain(verb);
      expect(stderr).toContain("agentsfleet library --help");
    },
  );

  test("no compatibility alias survived either spelling", async () => {
    const { stdout } = await help(["library", "--help"]);
    for (const [from, to] of RENAMED) {
      expect(stdout).toContain(to);
      // The retired verb must be gone from the listing, not merely deprioritised.
      expect(stdout).not.toMatch(new RegExp(`^\\s+${from}\\s`, "m"));
    }
  });
});

describe("every hint a library verb prints names a verb the parser accepts", () => {
  // The schedule rename moved the verbs and left the hints behind, so a bad
  // input answered with a sentence naming the one spelling the parser had
  // just stopped accepting. Each hint sits behind a credential check, so the
  // verbs run signed in against a mock API, and every `agentsfleet library
  // <verb>` they print must have a help page of its own.
  const ENTRY_ID = "01900000-0000-7000-8000-0000000aa001";
  const GALLERY_PATH = `/v1/workspaces/${WS_ID}/fleet-libraries`;
  const OWNED_PATH = `/v1/workspaces/${WS_ID}/library-entries`;
  const HINTED_VERB = /agentsfleet library ([a-z-]+)/g;
  const STATUS_OK = 200;
  const STATUS_NO_CONTENT = 204;
  const ENTRY = { id: ENTRY_ID, name: "github-pr-reviewer", visibility: "platform", source_kind: "github" };
  const listing = (items: ReadonlyArray<unknown>) => () => jsonResponse(STATUS_OK, { items });

  const signedIn = (argv: ReadonlyArray<string>, routes: MockRoutes) =>
    authedScope(() =>
      withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        out.stream.isTTY = true;
        await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) });
        return `${out.read()}${err.read()}`;
      }),
    );

  const cases: ReadonlyArray<readonly [string, ReadonlyArray<string>, MockRoutes]> = [
    ["an empty gallery", [LIBRARY], { [`GET ${GALLERY_PATH}`]: listing([]) }],
    ["an empty workspace", [LIBRARY, "list"], { [`GET ${OWNED_PATH}`]: listing([]) }],
    ["a listed entry", [LIBRARY, "list"], { [`GET ${OWNED_PATH}`]: listing([ENTRY]) }],
    ["a removed entry", [LIBRARY, "delete", ENTRY_ID], {
      [`DELETE ${OWNED_PATH}/${ENTRY_ID}`]: () => new Response(null, { status: STATUS_NO_CONTENT }),
    }],
    ["a create with no source", [LIBRARY, "create"], {}],
  ];

  test.each(cases)("%s", async (_label, argv, routes) => {
    const output = await signedIn(argv, routes);
    const hinted = [...output.matchAll(HINTED_VERB)].map(([, verb]) => verb ?? "");

    expect(hinted.length).toBeGreaterThan(0);
    // An unknown verb's --help falls back to the group's page, whose usage
    // line reads `library <subcommand>`; a real verb's page names itself.
    for (const verb of new Set(hinted)) {
      const { stdout } = await help([LIBRARY, verb, "--help"]);
      expect(stdout).toContain(`agentsfleet library ${verb} `);
    }
  });
});
