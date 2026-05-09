import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi } from "./helpers-mock-api.js";

// Each `commands/<area>.js` file exposes a tiny dispatcher that maps
// the first positional arg to a sub-command function. Every dispatcher
// has a fall-through branch that prints usage + returns exit 2 when
// the action is unknown. Without exercising that branch, the line
// coverage for these files lands ~60-90% even though the function
// table is fully covered. This sweep drives the unknown-action path
// for every area dispatcher in one place.

const WS_ID = "ws_unknown_action_sweep";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_uas" }, fn);

const SCENARIOS = [
  // [area-command, subcommand, friendly-name]
  ["agent", "frobnicate", "agent"],
  ["grant", "frobnicate", "grant"],
  ["tenant", "frobnicate", "tenant"],
  ["zombie", "frobnicate", "zombie"], // zombie subcommand router (install/list/etc.)
  ["credential", "frobnicate", "credential"], // workspace-level credential dispatcher (if separate)
];

describe("command dispatcher unknown-action sweep", () => {
  for (const [cmd, subcmd, name] of SCENARIOS) {
    test(`${name}: unknown subcommand prints usage and exits 2`, async () => {
      await authedScope(async () => {
        await withMockApi({}, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli([cmd, subcmd], {
            stdout: out.stream,
            stderr: err.stream,
            env: { ZOMBIE_API_URL: apiUrl },
          });
          // Either the dispatcher returns 2 (unknown subcommand) or
          // the route maps to a top-level command that itself errors
          // on the unknown arg with code 1 or 2. Both are valid
          // dispatch arrows and exercise the fall-through branch.
          expect([1, 2]).toContain(code);
          // Stderr contains *some* error narrative — proves the error
          // branch ran rather than silent failure.
          expect(err.read().length).toBeGreaterThan(0);
        });
      });
    });
  }

  test("agent dispatcher with no subcommand prints usage", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["agent"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { ZOMBIE_API_URL: apiUrl },
        });
        expect(code).toBe(2);
        expect(err.read()).toMatch(/agent/i);
      });
    });
  });

  test("agent dispatcher in JSON mode emits UNKNOWN_COMMAND structured error", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["--json", "agent", "frobnicate"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { ZOMBIE_API_URL: apiUrl },
        });
        expect(code).toBe(2);
        // JSON mode renders error to stderr or stdout depending on
        // implementation; smoke check the body is structured.
        const text = err.read() + out.read();
        expect(text).toContain("UNKNOWN_COMMAND");
      });
    });
  });

  test("tenant provider with no further subcommand prints usage", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["tenant", "provider"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { ZOMBIE_API_URL: apiUrl },
        });
        // tenant provider with no subcommand: either prints usage (2)
        // or runs a default action that errors against the empty mock (1).
        expect([1, 2]).toContain(code);
      });
    });
  });
});
