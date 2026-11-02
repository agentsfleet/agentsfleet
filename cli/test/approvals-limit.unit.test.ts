import { expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { fetchGates } from "../src/commands/approvals.ts";
import { ValidationError } from "../src/errors/index.ts";
import { HttpClient } from "../src/services/http-client.ts";

// The operator must be told when the bounded walk stops with unread gates.
const PAGE_CAP = 50;

test.each([true, false])("approval page cap reports whether more gates remain: %s", async (hasMore) => {
  let pages = 0;
  const http = Layer.succeed(HttpClient, {
    request: <T>() => Effect.sync(() => {
      pages++;
      return {items: [{gate_id: `gate-${pages}`}], next_cursor: pages < PAGE_CAP || hasMore ? `page-${pages}` : null} as T;
    }),
  });
  const result = await Effect.runPromiseExit(
    fetchGates("workspace", Redacted.make("test-token"), {}).pipe(Effect.provide(http)),
  );
  expect(pages).toBe(PAGE_CAP);
  expect(Exit.isFailure(result)).toBe(hasMore);
  if (Exit.isFailure(result)) {
    const error = Option.getOrNull(Cause.findErrorOption(result.cause));
    expect(error).toBeInstanceOf(ValidationError);
    expect(error).toMatchObject({detail: "approval list is incomplete after 50 pages", suggestion: "filter by fleet with --fleet <fleet_id> and retry"});
  } else expect(result.value).toHaveLength(PAGE_CAP);
});
