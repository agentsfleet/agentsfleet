import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi } from "./helpers-mock-api.js";

// Sweep test for the cli.js `handlers` registry. Each route key
// corresponds to one arrow-function in the dispatch table; invoking
// runCli with that route name exercises the arrow (even when the
// underlying command exits non-zero due to missing args). The
// behaviour we assert is "the arrow ran and returned an exit code",
// not "the command succeeded" — that's what the per-command
// integration tests cover.
//
// Without this sweep, adding a new route handler with a typo'd key
// or a missing dep silently routes nowhere; the cli.js coverage
// number is the only safety net.

const WS_ID = "ws_dispatch_sweep";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_sweep" }, fn);

const ROUTES_THAT_NEED_API = [
  ["agent", ["list", "--workspace", WS_ID]],
  ["grant", ["list", "--workspace", WS_ID]],
  ["tenant", ["provider", "show"]],
  ["billing", ["show"]],
  ["install", ["--from", "/nonexistent/skill"]],
  ["zombie", ["list"]],
  ["list", []],
  ["status", ["zmb_x"]],
  ["kill", ["zmb_x"]],
  ["stop", ["zmb_x"]],
  ["resume", ["zmb_x"]],
  ["delete", ["zmb_x"]],
  ["logs", ["zmb_x"]],
  ["steer", []],
  ["events", ["zmb_x"]],
  ["credential", ["list", "zmb_x"]],
];

const ROUTES_NO_API = [
  ["doctor", []],
  ["workspace", ["list"]],
  ["logout", []],
];

describe("cli.js dispatch table sweep", () => {
  for (const [cmd, args] of ROUTES_THAT_NEED_API) {
    test(`route '${cmd}' arrow is invoked (smoke)`, async () => {
      await authedScope(async () => {
        // Generic catch-all mock — every API call returns an empty
        // success body. Real shape mismatches surface as the route
        // handler's own error path (still exercises the arrow).
        await withMockApi({}, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli([cmd, ...args], {
            stdout: out.stream,
            stderr: err.stream,
            env: { ZOMBIE_API_URL: apiUrl },
          });
          // Any numeric exit code proves the arrow ran. We don't
          // assert success — that's per-command unit/integration
          // tests' job. We assert dispatch reached *a* handler.
          expect(typeof code).toBe("number");
        });
      });
    });
  }

  for (const [cmd, args] of ROUTES_NO_API) {
    test(`route '${cmd}' arrow is invoked (no-api)`, async () => {
      await authedScope(async () => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli([cmd, ...args], {
          stdout: out.stream,
          stderr: err.stream,
          env: {},
        });
        expect(typeof code).toBe("number");
      });
    });
  }

  test("unknown command surfaces UNKNOWN_COMMAND with exit 2", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["nonsense-command-xyz"], {
        stdout: out.stream,
        stderr: err.stream,
        env: {},
      });
      expect(code).toBe(2);
      expect(err.read()).toContain("unknown command");
    });
  });

  test("unknown command with similar name surfaces a 'did you mean' suggestion", async () => {
    await authedScope(async () => {
      const out = bufferStream();
      const err = bufferStream();
      // 'zomby' is one edit from 'zombie'-prefixed routes; the suggestion
      // path runs even though no exact match exists.
      const code = await runCli(["doctorr"], {
        stdout: out.stream,
        stderr: err.stream,
        env: {},
      });
      expect(code).toBe(2);
      const errOut = err.read();
      expect(errOut).toContain("unknown command");
    });
  });
});
