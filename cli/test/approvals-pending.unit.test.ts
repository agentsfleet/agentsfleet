// Unit coverage for src/commands/approvals_pending.ts — the lookup that turns a
// steer timeout into a next step, and the per-Fleet counts `status` renders.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer, Redacted } from "effect";

import {
  parkedGateHint,
  pendingGateCounts,
} from "../src/commands/approvals_pending.ts";
import { requireGateId } from "../src/commands/approvals.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const FLEET_ID = "01900000-0000-7000-8000-0000007670f7";
const OTHER_FLEET_ID = "01900000-0000-7000-8000-0000007670f8";
const GATE_ID = "01900000-0000-7000-8000-000000099a01";
const TOKEN = Redacted.make("test.jwt.approvals");

const gate = (overrides: Record<string, unknown> = {}) => ({
  gate_id: GATE_ID,
  fleet_id: FLEET_ID,
  status: "pending",
  ...overrides,
});

const httpLayer = (
  requests: HttpRequestInput[],
  response: unknown,
  fail = false,
): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: <T>(input: HttpRequestInput) => {
      requests.push(input);
      return fail
        ? (Effect.die(new Error("inbox unreachable")) as Effect.Effect<T, never, never>)
        : (Effect.sync(() => response as T) as Effect.Effect<T, never, never>);
    },
  });

const runHint = (response: unknown, requests: HttpRequestInput[] = []) =>
  Effect.runPromise(
    parkedGateHint(WS_ID, FLEET_ID, TOKEN).pipe(
      Effect.provide(httpLayer(requests, response)),
    ),
  );

describe("parkedGateHint", () => {
  test("names the single gate and the command that clears it", async () => {
    const hint = await runHint({ items: [gate()] });
    expect(hint).toContain(GATE_ID);
    expect(hint).toContain("agentsfleet approvals approve");
  });

  test("points at the list when more than one gate is waiting", async () => {
    const hint = await runHint({
      items: [gate(), gate({ gate_id: "01900000-0000-7000-8000-000000099a02" })],
    });
    expect(hint).toContain("2 approval gates");
    expect(hint).toContain("agentsfleet approvals list --fleet");
  });

  test("returns null when nothing is holding this Fleet", async () => {
    expect(await runHint({ items: [] })).toBeNull();
  });

  test("ignores gates belonging to another Fleet", async () => {
    expect(await runHint({ items: [gate({ fleet_id: OTHER_FLEET_ID })] })).toBeNull();
  });

  test("ignores gates that are already decided", async () => {
    expect(await runHint({ items: [gate({ status: "approved" })] })).toBeNull();
  });

  test("reads the workspace approvals inbox", async () => {
    const requests: HttpRequestInput[] = [];
    await runHint({ items: [] }, requests);
    expect(requests[0]?.path).toBe(`/v1/workspaces/${WS_ID}/approvals`);
  });

  test("an unreachable inbox yields no hint instead of a second failure", async () => {
    // The diagnosis must never replace the failure being diagnosed: a steer
    // that timed out still reports its timeout, with its ordinary suggestion.
    const hint = await Effect.runPromise(
      parkedGateHint(WS_ID, FLEET_ID, TOKEN).pipe(
        Effect.provide(httpLayer([], null, true)),
      ),
    );
    expect(hint).toBeNull();
  });
});

describe("pendingGateCounts", () => {
  test("counts pending gates per Fleet and omits decided ones", async () => {
    const counts = await Effect.runPromise(
      pendingGateCounts(WS_ID, TOKEN).pipe(
        Effect.provide(
          httpLayer([], {
            items: [
              gate(),
              gate({ gate_id: "01900000-0000-7000-8000-000000099a02" }),
              gate({ gate_id: "01900000-0000-7000-8000-000000099a03", fleet_id: OTHER_FLEET_ID }),
              gate({ gate_id: "01900000-0000-7000-8000-000000099a04", status: "denied" }),
            ],
          }),
        ),
      ),
    );
    expect(counts.get(FLEET_ID)).toBe(2);
    expect(counts.get(OTHER_FLEET_ID)).toBe(1);
  });

  test("a gate with no Fleet identifier is not counted against one", async () => {
    const counts = await Effect.runPromise(
      pendingGateCounts(WS_ID, TOKEN).pipe(
        Effect.provide(httpLayer([], { items: [gate({ fleet_id: null })] })),
      ),
    );
    expect(counts.size).toBe(0);
  });

  test("an unreachable inbox yields no counts rather than failing status", async () => {
    const counts = await Effect.runPromise(
      pendingGateCounts(WS_ID, TOKEN).pipe(
        Effect.provide(httpLayer([], null, true)),
      ),
    );
    expect(counts.size).toBe(0);
  });
});

describe("requireGateId", () => {
  test("accepts a supplied identifier", async () => {
    expect(await Effect.runPromise(requireGateId(GATE_ID))).toBe(GATE_ID);
  });

  test("refuses a missing identifier with the usage line", async () => {
    // commander declares `<gate_id>` required, so this arm is the type-level
    // guard behind that: the handler reads `positionals[0]`, which is
    // `string | undefined`, and must not send a request for `undefined`.
    const exit = await Effect.runPromiseExit(requireGateId(undefined));
    expect(Exit.isFailure(exit)).toBe(true);
    expect(JSON.stringify(exit)).toContain("agentsfleet approvals show");
  });
});
