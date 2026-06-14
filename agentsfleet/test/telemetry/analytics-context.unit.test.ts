// withAnalyticsContext / CurrentAnalyticsContext coverage. Mirrors
// Supabase's analytics-context.unit.test.ts, adapted for bun:test +
// Effect 4.0.0-beta.67.

import { describe, expect, it } from "bun:test";
import { Effect, Fiber } from "effect";
import {
  CurrentAnalyticsContext,
  withAnalyticsContext,
} from "../../src/services/telemetry/analytics-context.ts";

describe("withAnalyticsContext", () => {
  it("merges nested contexts and restores the outer value afterward", async () => {
    const program = Effect.gen(function* () {
      const before = yield* CurrentAnalyticsContext;
      expect(before).toEqual({});

      const nested = yield* Effect.gen(function* () {
        const current = yield* CurrentAnalyticsContext;
        return current;
      }).pipe(
        withAnalyticsContext({
          command_run_id: "run-123",
          groups: { organization: "usezombie" },
        }),
        withAnalyticsContext({
          command: "login",
          groups: { workspace: "ws-abc" },
        }),
      );

      expect(nested).toEqual({
        command_run_id: "run-123",
        command: "login",
        groups: { organization: "usezombie", workspace: "ws-abc" },
      });

      const after = yield* CurrentAnalyticsContext;
      expect(after).toEqual({});
    });
    await Effect.runPromise(program);
  });

  it("is inherited by forked child fibers", async () => {
    const program = Effect.gen(function* () {
      const child = yield* Effect.gen(function* () {
        const fiber = yield* Effect.forkChild(
          Effect.gen(function* () {
            return yield* CurrentAnalyticsContext;
          }),
        );
        return yield* Fiber.join(fiber);
      }).pipe(
        withAnalyticsContext({
          command_run_id: "run-456",
          command: "start",
        }),
      );

      expect(child).toEqual({
        command_run_id: "run-456",
        command: "start",
      });
    });
    await Effect.runPromise(program);
  });

  it("drops groups when neither incoming nor existing has them", async () => {
    const program = Effect.gen(function* () {
      const nested = yield* CurrentAnalyticsContext.pipe(
        Effect.flatMap((c) => Effect.succeed(c)),
      ).pipe(withAnalyticsContext({ command_run_id: "rid", command: "x" }));
      expect(nested.groups).toBeUndefined();
      expect(nested.command_run_id).toBe("rid");
    });
    await Effect.runPromise(program);
  });

  it("shallow-merges flag_values and flags_used", async () => {
    const program = Effect.gen(function* () {
      const ctx = yield* CurrentAnalyticsContext.pipe(
        Effect.flatMap((c) => Effect.succeed(c)),
      ).pipe(
        withAnalyticsContext({
          flags_used: ["a"],
          flag_values: { a: 1 },
        }),
        withAnalyticsContext({
          flags_used: ["b"],
          flag_values: { b: 2 },
        }),
      );
      // shallow replace, not array concat — outer wins
      expect(ctx.flags_used).toEqual(["a"]);
      expect(ctx.flag_values).toEqual({ a: 1 });
    });
    await Effect.runPromise(program);
  });
});
