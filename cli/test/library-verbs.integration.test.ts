// The library verbs reached from the parser rather than called directly:
// `library list` reads the workspace's own entries and `library delete`
// removes one, each through the command tree against a mock API. The
// effects themselves are graded in `library-entries.unit.test.ts`; this
// proves the tree hands each verb to its effect.

import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { removedNotice } from "../src/commands/fleet_library_delete.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { WS_ID, authedScope, withMockApi, jsonResponse, type MockRoutes } from "./helpers-fleet-install.ts";

const LIBRARY = "library";
const ENTRY_ID = "01900000-0000-7000-8000-0000000aa001";
const ENTRY_NAME = "github-pr-reviewer";
const OWNED_PATH = `/v1/workspaces/${WS_ID}/library-entries`;
const GALLERY_SEGMENT = "fleet-libraries";
const EXIT_OK = 0;
const STATUS_OK = 200;
const STATUS_NO_CONTENT = 204;

const run = (argv: readonly string[], routes: MockRoutes) =>
  authedScope(() =>
    withMockApi(routes, async (apiUrl, calls) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli([...argv], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      return { code, stdout: out.read(), stderr: err.read(), calls };
    }),
  );

describe("the library verbs, through the command tree", () => {
  test("library list reads the workspace's own entries, not the gallery", async () => {
    const { code, stdout, stderr, calls } = await run([LIBRARY, "list"], {
      [`GET ${OWNED_PATH}`]: () =>
        jsonResponse(STATUS_OK, {
          items: [
            {
              id: ENTRY_ID,
              name: ENTRY_NAME,
              description: "Reviews pull requests.",
              source_kind: "github",
              source_ref: "acme/reviewer",
              content_hash: "0123456789abcdef",
              created_at: 1_777_507_200_000,
            },
          ],
        }),
    });

    expect({ code, stderr }).toEqual({ code: EXIT_OK, stderr: "" });
    expect(stdout).toContain(ENTRY_NAME);
    expect(calls.some((call) => call.method === "GET" && call.path === OWNED_PATH)).toBe(true);
    expect(calls.some((call) => call.path.includes(GALLERY_SEGMENT))).toBe(false);
  });

  test("library delete removes the named entry and says fleets keep running", async () => {
    const { code, stdout, calls } = await run([LIBRARY, "delete", ENTRY_ID], {
      [`DELETE ${OWNED_PATH}/${ENTRY_ID}`]: () => new Response(null, { status: STATUS_NO_CONTENT }),
    });

    expect(code).toBe(EXIT_OK);
    expect(stdout).toContain(removedNotice(ENTRY_ID));
    expect(calls.map((call) => `${call.method} ${call.path}`)).toContain(
      `DELETE ${OWNED_PATH}/${ENTRY_ID}`,
    );
  });
});
